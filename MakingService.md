Com o serviço, o `serve` liga sozinho quando a raspi004 inicia e é reiniciado se cair. Tudo isto é na **raspi004**.

**1. Descubra onde está o binário**
```
which cofre
```
Anote o caminho. No exemplo abaixo uso `/home/pdsilva/.local/bin/cofre`; troque pelo que apareceu.

**2. Crie o arquivo do serviço**
```
sudo nano /etc/systemd/system/cofre-sync.service
```
Conteúdo:
```ini
[Unit]
Description=Servidor de sincronizacao do cofre
After=network-online.target
Wants=network-online.target

[Service]
User=pdsilva
WorkingDirectory=/home/pdsilva/cofre
Environment=COFRE_ARQ=/home/pdsilva/cofre/cofre.vault
Environment=COFRE_CHAVE=/home/pdsilva/cofre/sync.key
ExecStart=/home/pdsilva/.local/bin/cofre serve 4300
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=/home/pdsilva/cofre

[Install]
WantedBy=multi-user.target
```

**3. Ative e ligue**
```
sudo systemctl daemon-reload
sudo systemctl enable --now cofre-sync
```

**4. Confira**
```
systemctl status cofre-sync
journalctl -u cofre-sync -f
```
O status deve mostrar `active (running)`, e o `journalctl` mostra cada sincronização (`IP: ok` ou o erro). Saia do `journalctl` com `Ctrl-C`.

**5. Teste do PC**
```
cofre sync raspi004:4300
```

**Pontos de atenção**

- **Pare o `serve` manual antes**, se ainda estiver rodando em algum terminal, senão a porta 4300 fica ocupada e o serviço falha com "Address already in use".
- **`ReadWritePaths` precisa da pasta inteira**, não só do arquivo: o programa grava um `.tmp` ao lado do cofre e renomeia, e guarda o `.bak`. Se restringir ao arquivo, a gravação falha.
- **Permissões:** o serviço roda como `pdsilva` e precisa ler o `sync.key`. Se der erro de permissão, rode `chmod 600 ~/cofre/sync.key ~/cofre/cofre.vault` na raspi004.
- **Parar ou reiniciar:** `sudo systemctl stop cofre-sync` e `sudo systemctl restart cofre-sync`. Para desligar na inicialização: `sudo systemctl disable cofre-sync`.
- **Atualizar o programa:** depois de recompilar na raspi004 e copiar o binário novo, rode `sudo systemctl restart cofre-sync`.
- **Segurança:** o serviço escuta em todas as interfaces. Mantenha a porta 4300 liberada só para a rede local no firewall, e nunca para a internet.
- **Usar o cofre na raspi004 com o serviço ligado:** `cofre list` e `cofre get` funcionam normalmente. Só evite fazer `cofre add` lá no mesmo instante em que o PC está sincronizando.

Se o `status` mostrar `failed`, cole as últimas linhas do `journalctl -u cofre-sync -n 20` que eu indico o problema.