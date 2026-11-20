Um arquivo **binário próprio**, o `cofre.vault`, com um cabeçalho fixo seguido do conteúdo cifrado. Dentro da parte cifrada, os dados são **JSON**.

**Estrutura do arquivo**

| Bytes | Conteúdo |
|---|---|
| 0–3 | marca `RGVT`, para reconhecer o formato |
| 4 | versão do formato (1) |
| 5–16 | parâmetros do Argon2: memória, passadas e threads (3 números de 4 bytes) |
| 17–32 | salt aleatório (16 bytes) |
| 33–56 | nonce aleatório (24 bytes) |
| 57 em diante | dados cifrados com XChaCha20-Poly1305, mais 16 bytes de autenticação no final |

O cabeçalho (57 bytes) fica em claro, mas é autenticado: qualquer alteração nele, mesmo de 1 byte, faz a abertura falhar.

**O conteúdo, depois de decifrado**

É um JSON gerado pela biblioteca `serde_json`, mais ou menos assim:

```json
{"entradas":[
  {"id":"a1b2c3d4e5f60718","servico":"Banco do Brasil","usuario":"joao123",
   "senha":"...","nota":"...","atualizado":1795000000,"apagado":false}
]}
```

O JSON nunca é gravado em disco sem cifra: ele só existe na memória, e é apagado dela quando o programa termina de usá-lo.

**Por que esse formato**

- **Binário com cabeçalho:** o programa sabe que parâmetros usar para abrir o arquivo, então você pode mudar o custo do Argon2 no futuro sem quebrar cofres antigos.
- **JSON por dentro:** é fácil de evoluir (novos campos entram sem quebrar os antigos) e fácil de inspecionar em caso de emergência.
- **Um arquivo só:** o cofre inteiro é um único arquivo, que se copia, guarda em pendrive ou sincroniza sem depender de banco de dados.

**Ver na prática:** `xxd cofre.vault | head` mostra `RGVT` no início e depois só bytes sem sentido, que é o esperado de dado cifrado. Se aparecer texto legível, algo está errado.

Os arquivos auxiliares são: `cofre.bak` (cópia da versão anterior, gerada a cada gravação, também cifrada) e `sync.key` (32 bytes aleatórios puros, **sem** cifra, que servem só para autenticar a sincronização e por isso devem ter permissão restrita).


