use crate::cofre::{
    cifrar, decifrar, gravar_atomico, mesclar, normalizado, salvar, Cofre, HDR, MAGIC, MAX_ARQ,
};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    path::Path,
    time::Duration,
};

type HmacSha = Hmac<Sha256>;
const TIMEOUT: Duration = Duration::from_secs(15);

fn es<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn sha(b: &[u8]) -> [u8; 32] {
    Sha256::digest(b).into()
}

fn enviar_bloco(s: &mut TcpStream, d: &[u8]) -> Result<(), String> {
    s.write_all(&(d.len() as u32).to_be_bytes()).map_err(es)?;
    s.write_all(d).map_err(es)
}

fn ler_bloco(s: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut l = [0u8; 4];
    s.read_exact(&mut l).map_err(es)?;
    let n = u32::from_be_bytes(l) as usize;
    if n > MAX_ARQ {
        return Err("bloco grande demais".into()); // limite ANTES de alocar
    }
    let mut v = vec![0u8; n];
    s.read_exact(&mut v).map_err(es)?;
    Ok(v)
}

// ------------------------------------------------------------------ servidor
fn atender(mut s: TcpStream, chave: &[u8], arq: &Path) -> Result<(), String> {
    s.set_read_timeout(Some(TIMEOUT)).ok();
    s.set_write_timeout(Some(TIMEOUT)).ok();

    let mut desafio = [0u8; 32];
    getrandom::getrandom(&mut desafio).map_err(es)?;
    s.write_all(&desafio).map_err(es)?;
    let mut resp = [0u8; 32];
    s.read_exact(&mut resp).map_err(es)?;
    let mut m = HmacSha::new_from_slice(chave).map_err(es)?;
    m.update(&desafio);
    if m.verify_slice(&resp).is_err() {
        return Err("autenticação falhou".into()); // comparação em tempo constante
    }

    loop {
        let mut cmd = [0u8; 1];
        if s.read(&mut cmd).map_err(es)? == 0 {
            return Ok(()); // cliente fechou
        }
        match cmd[0] {
            b'G' => {
                let d = fs::read(arq).unwrap_or_default();
                enviar_bloco(&mut s, &d)?;
            }
            b'P' => {
                let mut esperado = [0u8; 32];
                s.read_exact(&mut esperado).map_err(es)?;
                let novo = ler_bloco(&mut s)?;
                if novo.len() < HDR + 16 || novo[0..4] != MAGIC[..] {
                    return Err("bloco recebido não é um cofre".into());
                }
                let atual = fs::read(arq).unwrap_or_default();
                let sha_atual = if atual.is_empty() { [0u8; 32] } else { sha(&atual) };
                if sha_atual != esperado {
                    s.write_all(b"C").map_err(es)?; // conflito: mudou desde o GET
                    continue;
                }
                gravar_atomico(arq, &novo)?;
                s.write_all(b"O").map_err(es)?;
            }
            _ => return Err("comando desconhecido".into()),
        }
    }
}

pub fn servir(porta: u16, chave: &[u8], arq: &Path) -> Result<(), String> {
    let l = TcpListener::bind(("0.0.0.0", porta)).map_err(es)?;
    println!("Servindo {} na porta {} (Ctrl-C para parar)", arq.display(), porta);
    for conn in l.incoming() {
        match conn {
            Ok(s) => {
                let quem = s.peer_addr().map(|a| a.to_string()).unwrap_or_default();
                match atender(s, chave, arq) {
                    Ok(()) => println!("{}: ok", quem),
                    Err(e) => eprintln!("{}: {}", quem, e),
                }
            }
            Err(e) => eprintln!("accept: {}", e),
        }
    }
    Ok(())
}

// ------------------------------------------------------------------- cliente
pub fn sincronizar(
    addr: &str,
    chave: &[u8],
    local: &mut Cofre,
    mestre: &str,
    arq: &Path,
) -> Result<String, String> {
    let sa = addr
        .to_socket_addrs()
        .map_err(es)?
        .next()
        .ok_or_else(|| "endereço inválido".to_string())?;
    let mut s = TcpStream::connect_timeout(&sa, Duration::from_secs(5))
        .map_err(|e| format!("não conectou: {}", e))?;
    s.set_read_timeout(Some(TIMEOUT)).ok();
    s.set_write_timeout(Some(TIMEOUT)).ok();

    let mut desafio = [0u8; 32];
    s.read_exact(&mut desafio).map_err(es)?;
    let mut m = HmacSha::new_from_slice(chave).map_err(es)?;
    m.update(&desafio);
    s.write_all(&m.finalize().into_bytes()).map_err(es)?;

    s.write_all(b"G").map_err(es)?;
    let remoto_bytes = ler_bloco(&mut s)
        .map_err(|e| format!("erro de rede (sync.key diferente nas duas máquinas?): {}", e))?;

    let remoto = if remoto_bytes.is_empty() {
        None
    } else {
        Some(decifrar(&remoto_bytes, mestre).map_err(|e| format!("cópia remota: {}", e))?)
    };

    let antes = normalizado(local);
    if let Some(r) = &remoto {
        mesclar(local, r);
    }
    let mudou_local = normalizado(local) != antes;
    if mudou_local {
        salvar(arq, local, mestre)?;
    }

    let mudou_remoto = match &remoto {
        None => true,
        Some(r) => normalizado(r) != normalizado(local),
    };
    if mudou_remoto {
        let novo = cifrar(local, mestre)?;
        let esperado = if remoto_bytes.is_empty() { [0u8; 32] } else { sha(&remoto_bytes) };
        s.write_all(b"P").map_err(es)?;
        s.write_all(&esperado).map_err(es)?;
        enviar_bloco(&mut s, &novo)?;
        let mut r = [0u8; 1];
        s.read_exact(&mut r).map_err(es)?;
        if r[0] != b'O' {
            return Err("a cópia remota mudou durante a sincronização; rode de novo".into());
        }
    }
    Ok(format!(
        "Sincronizado. Local atualizado: {}. Remoto atualizado: {}.",
        if mudou_local { "sim" } else { "não" },
        if mudou_remoto { "sim" } else { "não" }
    ))
}