use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Algorithm, Argon2, Params, Version,
};
use std::{fs, path::Path, thread, time::Duration};
use zeroize::Zeroizing;

const MIN_LEN: usize = 12;
const MAX_LEN: usize = 128; // limite de entrada: evita senha gigante forçando Argon2 a trabalhar à toa
const MAX_TENTATIVAS: u32 = 5;

// Argon2id: 64 MiB de memória, 3 passadas, 1 thread. Ajuste ao seu PC
// (alvo: ~0,3 a 1 s por verificação).
fn argon() -> Argon2<'static> {
    let params = Params::new(64 * 1024, 3, 1, None).expect("parâmetros Argon2 inválidos");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// Regras de força. Devolve a lista de problemas encontrados.
pub fn validar_forca(s: &str) -> Result<(), Vec<&'static str>> {
    let mut erros = Vec::new();
    let n = s.chars().count();

    if n < MIN_LEN { erros.push("mínimo de 12 caracteres"); }
    if s.len() > MAX_LEN { erros.push("máximo de 128 bytes"); }
    if !s.chars().any(|c| c.is_lowercase()) { erros.push("falta letra minúscula"); }
    if !s.chars().any(|c| c.is_uppercase()) { erros.push("falta letra maiúscula"); }
    if !s.chars().any(|c| c.is_ascii_digit()) { erros.push("falta número"); }
    if !s.chars().any(|c| !c.is_alphanumeric() && !c.is_whitespace()) { erros.push("falta símbolo"); }

    // repetição simples ("aaaaaaaaaaaa", "111111111111")
    let mut cs = s.chars();
    if let Some(p) = cs.next() {
        if n > 1 && s.chars().all(|c| c == p) { erros.push("só caracteres repetidos"); }
    }
    // lista mínima de senhas comuns; amplie ou carregue de arquivo
    let baixa = s.to_lowercase();
    for ruim in ["password", "senha", "qwerty", "123456", "admin", "letmein"] {
        if baixa.contains(ruim) { erros.push("contém padrão comum"); break; }
    }

    if erros.is_empty() { Ok(()) } else { Err(erros) }
}

/// Gera o hash PHC (inclui algoritmo, parâmetros e salt aleatório).
pub fn hash_senha(s: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    argon()
        .hash_password(s.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| format!("erro ao gerar hash: {}", e))
}

/// Confere a senha. A comparação interna é em tempo constante.
pub fn verificar(s: &str, hash: &str) -> bool {
    if s.len() > MAX_LEN { return false; }
    match PasswordHash::new(hash) {
        Ok(h) => argon().verify_password(s.as_bytes(), &h).is_ok(),
        Err(_) => false,
    }
}

fn ler(prompt: &str) -> Result<Zeroizing<String>, String> {
    rpassword::prompt_password(prompt)
        .map(Zeroizing::new) // apaga da memória ao sair do escopo
        .map_err(|e| format!("erro ao ler senha: {}", e))
}

fn salvar(arquivo: &Path, hash: &str) -> Result<(), String> {
    fs::write(arquivo, hash).map_err(|e| format!("erro ao gravar {}: {}", arquivo.display(), e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(arquivo, fs::Permissions::from_mode(0o600)).ok(); // só o dono lê
    }
    Ok(())
}

/// Chame no início do programa. Ok(()) = pode continuar.
/// Primeira execução (sem arquivo): cadastra a senha.
pub fn autenticar(arquivo: &Path) -> Result<(), String> {
    if !arquivo.exists() {
        println!("Primeiro uso: defina uma senha.");
        loop {
            let s1 = ler("Nova senha: ")?;
            if let Err(erros) = validar_forca(&s1) {
                println!("Senha fraca: {}", erros.join("; "));
                continue;
            }
            let s2 = ler("Repita a senha: ")?;
            if *s1 != *s2 { println!("As senhas não conferem."); continue; }
            salvar(arquivo, &hash_senha(&s1)?)?;
            println!("Senha cadastrada.");
            return Ok(());
        }
    }

    let hash = fs::read_to_string(arquivo)
        .map_err(|e| format!("erro ao ler {}: {}", arquivo.display(), e))?;
    let hash = hash.trim();

    for tentativa in 1..=MAX_TENTATIVAS {
        let s = ler("Senha: ")?;
        if verificar(&s, hash) { return Ok(()); }
        println!("Senha incorreta ({}/{}).", tentativa, MAX_TENTATIVAS);
        // atraso crescente: 1, 2, 4, 8 s... (máx. 16 s)
        thread::sleep(Duration::from_secs(1u64 << (tentativa - 1).min(4)));
    }
    Err("tentativas esgotadas".into())
}