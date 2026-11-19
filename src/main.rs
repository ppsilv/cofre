#[allow(dead_code)] // só usamos validar_forca; o resto do módulo antigo fica sem uso
mod senha;
mod cofre;
mod sinc;

use cofre::{agora, decifrar, novo_id, salvar, Cofre, Entrada};
use std::{env, fs, path::{Path, PathBuf}, process::ExitCode};
use zeroize::Zeroizing;
use std::io::{self, Write};

const MAX_NOTA: usize = 1000; // limite de caracteres da observação

const USO: &str = "uso:
  cofre init                      cria o cofre
  cofre add SERVICO USUARIO       adiciona/atualiza (a senha é pedida sem eco)
  cofre list                      lista serviços e usuários
  cofre get TEXTO                 mostra senhas dos serviços que contêm TEXTO
  cofre del SERVICO USUARIO       apaga
  cofre initsync                  gera a chave de sincronização (sync.key)
  cofre serve PORTA               serve a cópia deste cofre pela rede
  cofre sync IP:PORTA             sincroniza com o servidor
variáveis: COFRE_ARQ (padrão cofre.vault), COFRE_CHAVE (padrão sync.key)";

fn caminho(var: &str, padrao: &str) -> PathBuf {
    PathBuf::from(env::var(var).unwrap_or_else(|_| padrao.into()))
}

fn ler_senha(p: &str) -> Result<Zeroizing<String>, String> {
    rpassword::prompt_password(p)
        .map(Zeroizing::new)
        .map_err(|e| e.to_string())
}

// Lê uma linha com eco (para a observação, que não é segredo de digitação)
fn ler_linha(p: &str) -> Result<String, String> {
    print!("{}", p);
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut s = String::new();
    io::stdin().read_line(&mut s).map_err(|e| e.to_string())?;
    Ok(s.trim_end_matches(&['\r', '\n'][..]).to_string())
}

fn abrir(arq: &Path) -> Result<(Cofre, Zeroizing<String>), String> {
    let dados = fs::read(arq)
        .map_err(|e| format!("não consegui ler {}: {} (use 'init' na primeira vez)", arq.display(), e))?;
    let mestre = ler_senha("Senha mestra: ")?;
    let c = decifrar(&dados, &mestre)?;
    Ok((c, mestre))
}

fn init(arq: &Path) -> Result<(), String> {
    if arq.exists() {
        return Err(format!("{} já existe", arq.display()));
    }
    loop {
        let s1 = ler_senha("Nova senha mestra: ")?;
        if let Err(erros) = senha::validar_forca(&s1) {
            println!("Senha fraca: {}", erros.join("; "));
            continue;
        }
        let s2 = ler_senha("Repita: ")?;
        if *s1 != *s2 {
            println!("As senhas não conferem.");
            continue;
        }
        salvar(arq, &Cofre::default(), &s1)?;
        println!("Cofre criado. Se esquecer a senha mestra, NÃO há como recuperar.");
        return Ok(());
    }
}

fn add(arq: &Path, servico: &str, usuario: &str) -> Result<(), String> {
    let (mut c, mestre) = abrir(arq)?;
    let nova = ler_senha(&format!("Senha de {}/{}: ", servico, usuario))?;
    let nota = ler_linha("Observações (Enter = deixar como está, - = limpar): ")?;
    if nota.chars().count() > MAX_NOTA {
        return Err(format!("observação com mais de {} caracteres", MAX_NOTA));
    }
    let pos = c
        .entradas
        .iter()
        .position(|e| !e.apagado && e.servico == servico && e.usuario == usuario);
    match pos {
        Some(i) => {
            c.entradas[i].senha = nova.to_string();
            if nota == "-" {
                c.entradas[i].nota.clear();
            } else if !nota.is_empty() {
                c.entradas[i].nota = nota;
            }
            c.entradas[i].atualizado = agora();
        }
        None => c.entradas.push(Entrada {
            id: novo_id(),
            servico: servico.into(),
            usuario: usuario.into(),
            senha: nova.to_string(),
            nota: if nota == "-" { String::new() } else { nota },
            atualizado: agora(),
            apagado: false,
        }),
    }
    salvar(arq, &c, &mestre)
}

fn listar(arq: &Path) -> Result<(), String> {
    let (c, _) = abrir(arq)?;
    for e in c.entradas.iter().filter(|e| !e.apagado) {
        println!("{}  ({})", e.servico, e.usuario);
    }
    Ok(())
}

fn obter(arq: &Path, texto: &str) -> Result<(), String> {
    let (c, _) = abrir(arq)?;
    let q = texto.to_lowercase();
    let mut achou = false;
    for e in c.entradas.iter().filter(|e| !e.apagado && e.servico.to_lowercase().contains(&q)) {
        println!("Site/App : {}", e.servico);
        println!("Usuário  : {}", e.usuario);
        println!("Senha    : {}", e.senha);
        if !e.nota.is_empty() {
            println!("Obs      : {}", e.nota);
        }
        println!();
        achou = true;
    }
    if achou { Ok(()) } else { Err("nada encontrado".into()) }
}

fn apagar(arq: &Path, servico: &str, usuario: &str) -> Result<(), String> {
    let (mut c, mestre) = abrir(arq)?;
    let pos = c
        .entradas
        .iter()
        .position(|e| !e.apagado && e.servico == servico && e.usuario == usuario)
        .ok_or_else(|| "entrada não encontrada".to_string())?;
    // marca em vez de remover, para a exclusão se propagar na sincronização
    c.entradas[pos].apagado = true;
    c.entradas[pos].senha.clear();
    c.entradas[pos].nota.clear();
    c.entradas[pos].atualizado = agora();
    salvar(arq, &c, &mestre)
}

fn ler_chave(p: &Path) -> Result<Vec<u8>, String> {
    let k = fs::read(p).map_err(|e| format!("{}: {} (rode 'initsync')", p.display(), e))?;
    if k.len() != 32 {
        return Err("sync.key deve ter exatamente 32 bytes".into());
    }
    Ok(k)
}

fn init_sync(p: &Path) -> Result<(), String> {
    if p.exists() {
        return Err(format!("{} já existe", p.display()));
    }
    let mut k = [0u8; 32];
    getrandom::getrandom(&mut k).map_err(|e| e.to_string())?;
    cofre::gravar_atomico(p, &k)?;
    println!("Gerado {}. Copie este arquivo UMA vez para a outra máquina (scp ou pendrive).", p.display());
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let arq = caminho("COFRE_ARQ", "cofre.vault");
    let arqchave = caminho("COFRE_CHAVE", "sync.key");
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("");
    // O número de argumentos é conferido ANTES de indexar args[n].
    match (cmd, args.len()) {
        ("init", 2) => init(&arq),
        ("add", 4) => add(&arq, &args[2], &args[3]),
        ("list", 2) => listar(&arq),
        ("get", 3) => obter(&arq, &args[2]),
        ("del", 4) => apagar(&arq, &args[2], &args[3]),
        ("initsync", 2) => init_sync(&arqchave),
        ("serve", 3) => {
            let porta: u16 = args[2].parse().map_err(|_| "porta inválida".to_string())?;
            sinc::servir(porta, &ler_chave(&arqchave)?, &arq)
        }
        ("sync", 3) => {
            let chave = ler_chave(&arqchave)?;
            let (mut c, mestre) = abrir(&arq)?;
            println!("{}", sinc::sincronizar(&args[2], &chave, &mut c, &mestre, &arq)?);
            Ok(())
        }
        _ => Err(USO.into()),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Erro: {}", e);
            ExitCode::FAILURE
        }
    }
}