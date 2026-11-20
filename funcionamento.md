Como usar (passo a passo)
Máquina A: cofre init, depois cofre add gmail meuemail@gmail.com, e cofre initsync.
Copie o sync.key para a máquina B uma única vez, por scp ou pendrive. Não mande por e-mail nem chat.
Máquina B: COFRE_ARQ=~/cofre-copia.vault cofre serve 4300. Esta máquina não precisa da senha mestra.
Máquina A: cofre sync IP_DE_B:4300.

Na primeira sincronização o remoto está vazio e o cofre é enviado inteiro. Depois disso, cada sync mescla os dois lados. Se as senhas mestras forem diferentes nas duas máquinas, aparece “cópia remota: senha mestra errada” e nada é alterado.

Para testar sem a segunda máquina, use duas pastas na mesma e 127.0.0.1:4300, com COFRE_ARQ diferente em cada uma.

Limites que você deve conhecer
Sem recuperação. Esquecer a senha mestra significa perder tudo. Faça uma cópia do .vault num pendrive guardado em outro lugar. Ele é seguro para guardar porque está cifrado.
Rede. O conteúdo viaja já cifrado, mas o canal não tem TLS. Quem estiver no meio do caminho não lê as senhas e não forja nada, mas poderia descartar ou repetir uma versão antiga do cofre. Na sua rede local isso é aceitável. Fora dela, passe por um túnel (ssh -L 4300:localhost:4300 usuario@B) ou VPN, e nunca abra essa porta direto para a internet.
Relógio. Quem vence um conflito é o atualizado mais recente, então os dois PCs precisam estar com a hora certa (NTP). Se você editar a mesma entrada nas duas máquinas antes de sincronizar, a de hora maior ganha.
Memória. Enquanto o programa roda, as senhas ficam em String comuns, que não são apagadas da RAM. O programa é curto e termina logo, mas se alguém tirar um dump de memória nesse intervalo, as senhas aparecem.
get imprime a senha na tela. Ela fica no histórico de rolagem do terminal. Uma melhoria futura é copiar para a área de transferência e limpar depois de 20 s.
Argon2 leva ~1 s por abertura e por gravação, de propósito. Se ficar lento no seu PC, reduza M_COST em cofre.rs. Os cofres já gravados continuam abrindo, porque os parâmetros ficam dentro do arquivo.

Se compilar com erro, cole só a primeira mensagem do cargo build. Os pontos mais prováveis são a versão de alguma crate e o #[allow(dead_code)] no mod senha. Depois posso acrescentar um gerador de senhas aleatórias ou uma interface com o Slint.



Cria na pasta onde você está rodando o comando (a pasta atual do terminal), com o nome cofre.vault.

Isso vem desta linha do main.rs:

rust
let arq = caminho("COFRE_ARQ", "cofre.vault");

Ou seja, se a variável de ambiente COFRE_ARQ não existir, usa cofre.vault no diretório atual. O mesmo vale para a chave de sincronização, que fica em sync.key na pasta atual.

Cuidado com isso: se você rodar cofre add de outra pasta, ele não acha o cofre e diz “não consegui ler cofre.vault”. O cofre “mora” onde você o criou.

Para ter um lugar fixo, defina o caminho completo uma vez no ~/.bashrc (ou ~/.zshrc):

export COFRE_ARQ=$HOME/.cofre/cofre.vault
export COFRE_CHAVE=$HOME/.cofre/sync.key

e crie a pasta com permissão restrita:

mkdir -p ~/.cofre && chmod 700 ~/.cofre

Depois abra um terminal novo (ou rode source ~/.bashrc) e use cofre init de qualquer lugar. Assim o cofre e a chave ficam sempre em ~/.cofre/, e o sync e o serve também usam esses mesmos caminhos.

Se já criou um cofre.vault em outra pasta, é só mover o arquivo para ~/.cofre/ com mv, sem precisar recriar.











Os quatro campos já existem no cofre. A struct `Entrada` tem `servico`, `usuario`, `senha` e `nota`, e tudo isso é gravado dentro do arquivo cifrado. O que falta é o comando `add` pedir a observação (hoje ela fica sempre vazia) e o `get` mostrá-la. Só o `main.rs` muda, e os cofres que você já criou continuam abrindo.

**1. No topo do `main.rs`**, junto dos outros `use`:

```rust
use std::io::{self, Write};

const MAX_NOTA: usize = 1000; // limite de caracteres da observação
```

**2. Nova função** (pode ficar antes do `fn abrir`):

```rust
// Lê uma linha com eco (para a observação, que não é segredo de digitação)
fn ler_linha(p: &str) -> Result<String, String> {
    print!("{}", p);
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut s = String::new();
    io::stdin().read_line(&mut s).map_err(|e| e.to_string())?;
    Ok(s.trim_end_matches(&['\r', '\n'][..]).to_string())
}
```

**3. Substitua a função `add` inteira:**

```rust
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
```

**4. Dentro da função `obter`**, troque o `println!` do laço por:

```rust
        println!("Site/App : {}", e.servico);
        println!("Usuário  : {}", e.usuario);
        println!("Senha    : {}", e.senha);
        if !e.nota.is_empty() {
            println!("Obs      : {}", e.nota);
        }
        println!();
```

**Uso:**

```
cofre add "Banco do Brasil" joao123
Senha de Banco do Brasil/joao123: (sem eco)
Observações (...): conta corrente, token no celular
```

**Pontos de atenção:**

- **Nomes com espaço** precisam de aspas no terminal, como `"Banco do Brasil"`. Sem aspas, o shell entende como argumentos separados e o programa mostra o texto de uso.
- **Atualizar uma entrada:** rode `add` de novo com o mesmo site e usuário. A senha é substituída, e a observação só muda se você digitar algo (Enter mantém a antiga, `-` apaga).
- **Limite de 1000 caracteres** na observação, para o cofre não crescer sem controle. Se precisar de mais, aumente `MAX_NOTA`.
- **A observação é tão secreta quanto a senha.** Ela vai dentro do mesmo bloco cifrado, então pode guardar coisas como "pergunta de segurança: nome do primeiro cachorro". O `list` continua mostrando só site e usuário, sem expor observações.
- **Buscar por usuário:** o `get` procura só pelo nome do site. Se quiser achar também pelo usuário, é uma mudança de uma linha no filtro, e eu ajusto se pedir.




Tudo bem: a pasta `~/.cofre` era só uma sugestão minha, e o programa não a cria sozinho. Os seus arquivos estão na pasta onde você rodou `cofre init` e `cofre initsync`.

**1. Ache onde eles estão** (no PC):
```
find ~ \( -name "cofre.vault" -o -name "sync.key" \) -not -path "*/target/*" 2>/dev/null
```

**2. Se aparecerem os dois**, você pode usá-los onde estão. Basta rodar o `cofre sync` a partir dessa mesma pasta:
```
cd /pasta/onde/estao
cofre sync IP_ARM:4300
```
E copiar a chave para a ARM com o caminho que o `find` mostrou:
```
scp /pasta/onde/estao/sync.key usuario@IP_ARM:~/.cofre/sync.key
```
(a pasta `~/.cofre` precisa existir na ARM: `mkdir -p ~/.cofre && chmod 700 ~/.cofre`)

**3. Se quiser arrumar de vez no PC**, crie a pasta e mova os arquivos:
```
mkdir -p ~/.cofre && chmod 700 ~/.cofre
mv /pasta/onde/estao/cofre.vault /pasta/onde/estao/sync.key ~/.cofre/
```
e acrescente no `~/.bashrc`:
```
export COFRE_ARQ=$HOME/.cofre/cofre.vault
export COFRE_CHAVE=$HOME/.cofre/sync.key
```
Depois `source ~/.bashrc`, e os comandos funcionam de qualquer pasta.

**4. Se o `find` achar só o `cofre.vault` e não o `sync.key`**, falta gerar a chave. Rode `cofre initsync` (na pasta do cofre, ou com `COFRE_CHAVE` definida) e depois copie para a ARM.

**5. Se o `find` não achar nenhum dos dois**, o cofre ainda não foi criado nesse PC. Rode `cofre init`, e depois `cofre initsync`.

Me diga o que o `find` mostrou, se algo não bater.








Não. Os dois comandos servem para criar os arquivos, e na ARM eles já existem, copiados do PC. Rodar `cofre init` lá seria inútil, e criar um cofre novo na ARM seria até prejudicial.

- **`cofre init`** recusa se o `cofre.vault` já existe ("já existe"), então não faria nada. Só deixaria de recusar se você apagasse o arquivo, e aí criaria um cofre vazio, diferente do seu.
- **`cofre initsync`** também recusa se o `sync.key` já existe. Se gerasse outra chave, ela seria diferente da do PC, e a sincronização falharia na autenticação.

O que importa é que os dois arquivos na ARM sejam **idênticos** aos do PC. Confira rodando nas duas máquinas, dentro de `~/cofre`:
```
sha256sum sync.key
```
O resultado tem que ser igual. Para o `cofre.vault` não precisa conferir, porque o próprio `sync` mescla e acerta as diferenças.

**Como usar daqui para a frente:**

1. Na ARM, dentro de `~/cofre`:
   ```
   ./target/release/cofre serve 4300
   ```
   (ou só `cofre serve 4300` se o binário estiver no PATH)
2. No PC, dentro de `~/cofre`:
   ```
   cofre sync IP_ARM:4300
   ```
   Como os dois cofres são iguais, deve responder "Local atualizado: não. Remoto atualizado: não".
3. `Ctrl-C` na ARM para parar o servidor.

Lembre que, sem `COFRE_ARQ` e `COFRE_CHAVE` definidas, o programa procura `cofre.vault` e `sync.key` na **pasta atual**. Por isso rode os comandos de dentro de `~/cofre` nas duas máquinas, ou defina as variáveis no `~/.bashrc`.

Para testar que a sincronização funciona de verdade: faça um `cofre add` no PC, rode o `sync`, e depois um `cofre list` na ARM. A entrada nova deve aparecer lá.