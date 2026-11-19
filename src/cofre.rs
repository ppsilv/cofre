use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

pub const MAGIC: &[u8; 4] = b"RGVT";
const VERSAO: u8 = 1;
// cabeçalho: magic(4) versao(1) m(4) t(4) p(4) salt(16) nonce(24)
pub const HDR: usize = 4 + 1 + 12 + 16 + 24;
const M_COST: u32 = 256 * 1024; // em KiB = 256 MiB
const T_COST: u32 = 3;
const P_COST: u32 = 1;
// Limites ao LER o cabeçalho: o arquivo é dado externo, não confie nele.
const M_MAX: u32 = 1024 * 1024; // 1 GiB
const T_MAX: u32 = 10;
const P_MAX: u32 = 8;
pub const MAX_ARQ: usize = 16 * 1024 * 1024;

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct Entrada {
    pub id: String,
    pub servico: String,
    pub usuario: String,
    pub senha: String,
    pub nota: String,
    pub atualizado: u64, // segundos Unix
    pub apagado: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct Cofre {
    pub entradas: Vec<Entrada>,
}

fn es<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

pub fn agora() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn novo_id() -> String {
    let mut b = [0u8; 8];
    getrandom::getrandom(&mut b).expect("sem gerador aleatório do sistema");
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

fn derivar(senha: &str, salt: &[u8], m: u32, t: u32, p: u32) -> Result<Zeroizing<[u8; 32]>, String> {
    let params = Params::new(m, t, p, Some(32)).map_err(es)?;
    let a = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut k = Zeroizing::new([0u8; 32]);
    a.hash_password_into(senha.as_bytes(), salt, &mut *k).map_err(es)?;
    Ok(k)
}

pub fn cifrar(c: &Cofre, senha: &str) -> Result<Vec<u8>, String> {
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 24];
    getrandom::getrandom(&mut salt).map_err(es)?;
    getrandom::getrandom(&mut nonce).map_err(es)?;

    let mut hdr = Vec::with_capacity(HDR);
    hdr.extend_from_slice(MAGIC);
    hdr.push(VERSAO);
    hdr.extend_from_slice(&M_COST.to_be_bytes());
    hdr.extend_from_slice(&T_COST.to_be_bytes());
    hdr.extend_from_slice(&P_COST.to_be_bytes());
    hdr.extend_from_slice(&salt);
    hdr.extend_from_slice(&nonce);

    let chave = derivar(senha, &salt, M_COST, T_COST, P_COST)?;
    let json = Zeroizing::new(serde_json::to_vec(c).map_err(es)?);
    let cif = XChaCha20Poly1305::new_from_slice(&chave[..]).map_err(es)?;
    // O cabeçalho entra como AAD: é autenticado, qualquer alteração derruba a abertura.
    let ct = cif
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: &json[..], aad: &hdr })
        .map_err(|_| "falha ao cifrar".to_string())?;
    hdr.extend_from_slice(&ct);
    Ok(hdr)
}

pub fn decifrar(dados: &[u8], senha: &str) -> Result<Cofre, String> {
    if dados.len() < HDR + 16 || dados.len() > MAX_ARQ {
        return Err("arquivo de cofre com tamanho inválido".into());
    }
    if dados[0..4] != MAGIC[..] || dados[4] != VERSAO {
        return Err("formato de cofre desconhecido".into());
    }
    let u32_em = |i: usize| u32::from_be_bytes([dados[i], dados[i + 1], dados[i + 2], dados[i + 3]]);
    let (m, t, p) = (u32_em(5), u32_em(9), u32_em(13));
    if p == 0 || p > P_MAX || t == 0 || t > T_MAX || m < 8 * p || m > M_MAX {
        return Err("parâmetros do cabeçalho fora do limite".into());
    }
    let salt = &dados[17..33];
    let nonce = &dados[33..57];

    let chave = derivar(senha, salt, m, t, p)?;
    let cif = XChaCha20Poly1305::new_from_slice(&chave[..]).map_err(es)?;
    let json = Zeroizing::new(
        cif.decrypt(
            XNonce::from_slice(nonce),
            Payload { msg: &dados[HDR..], aad: &dados[..HDR] },
        )
        .map_err(|_| "senha mestra errada ou arquivo corrompido".to_string())?,
    );
    serde_json::from_slice(&json).map_err(es)
}

/// Grava em .tmp, fsync e renomeia. Guarda o anterior como .bak.
pub fn gravar_atomico(arq: &Path, dados: &[u8]) -> Result<(), String> {
    let tmp = arq.with_extension("tmp");
    let mut oo = fs::OpenOptions::new();
    oo.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        oo.mode(0o600); // só o dono lê
    }
    let mut f = oo.open(&tmp).map_err(|e| format!("{}: {}", tmp.display(), e))?;
    f.write_all(dados).map_err(es)?;
    f.sync_all().map_err(es)?;
    drop(f);
    if arq.exists() {
        fs::copy(arq, arq.with_extension("bak")).ok();
    }
    fs::rename(&tmp, arq).map_err(|e| format!("{}: {}", arq.display(), e))
}

pub fn salvar(arq: &Path, c: &Cofre, senha: &str) -> Result<(), String> {
    gravar_atomico(arq, &cifrar(c, senha)?)
}

/// Une 'remoto' em 'local': para cada id vence o de 'atualizado' maior (empate: local).
pub fn mesclar(local: &mut Cofre, remoto: &Cofre) {
    for r in &remoto.entradas {
        match local.entradas.iter().position(|e| e.id == r.id) {
            Some(i) => {
                if r.atualizado > local.entradas[i].atualizado {
                    local.entradas[i] = r.clone();
                }
            }
            None => local.entradas.push(r.clone()),
        }
    }
}

/// Cópia ordenada por id, para comparar dois cofres sem depender da ordem.
pub fn normalizado(c: &Cofre) -> Cofre {
    let mut n = c.clone();
    n.entradas.sort_by(|a, b| a.id.cmp(&b.id));
    n
}