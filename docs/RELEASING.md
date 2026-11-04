# Como publicar uma versão

As releases são publicadas no GitHub (`matheusdevai/forge-ade`) pelo workflow `.github/workflows/release.yml`.

## Uma vez só: chave de assinatura do atualizador

O atualizador só instala pacotes assinados com a chave do projeto (minisign):

- **Chave pública:** em `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`.
- **Chave privada:** `~/.tauri/forge-updater.key` (permissão 0600), **somente na máquina do mantenedor**.
- **Senha da chave:** no Keychain do macOS, serviço `dev.forge.updater.signing`, conta `forge-updater`.
- **Segredos do GitHub Actions:**
  - `TAURI_SIGNING_PRIVATE_KEY`: o conteúdo do arquivo da chave privada;
  - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: a senha.

> **Se a chave privada for perdida, os apps instalados não aceitam mais atualizações.** Será preciso distribuir uma nova versão manualmente com uma nova chave pública. Faça backup da chave e da senha num cofre seguro.

## Publicar

1. Atualize a versão em `package.json`, `src-tauri/Cargo.toml` e `src-tauri/tauri.conf.json`.
2. Registre as mudanças em `CHANGELOG.md`.
3. Crie e envie a tag:

   ```sh
   git tag v0.2.0 && git push origin v0.2.0
   ```

4. O workflow compila para Apple Silicon e Intel, assina os pacotes de atualização e cria uma release **em rascunho** com o `.dmg`, o `.app.tar.gz`, o `.sig` e o `latest.json`.
5. Revise o rascunho e publique:

   ```sh
   gh release edit v0.2.0 --draft=false
   ```

   O atualizador consulta `releases/latest/download/latest.json`, então só enxerga releases publicadas.

## Sem notarização Apple

O projeto não tem conta Apple Developer. O app é assinado ad-hoc (com o entitlement de microfone), e por isso a primeira abertura exige "Abrir" manual (veja `docs/USER_GUIDE.md`). Para notarizar no futuro, configure `APPLE_CERTIFICATE`, `APPLE_ID`, `APPLE_PASSWORD` e `APPLE_TEAM_ID` no workflow e defina `bundle.macOS.signingIdentity` com o "Developer ID Application".
