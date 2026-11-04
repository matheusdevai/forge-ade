# Segurança

## Reportar uma vulnerabilidade

**Não abra uma issue pública.** Use o recurso *Report a vulnerability* (GitHub Security Advisories) do repositório `matheusdevai/forge-ade`, com os passos para reproduzir.

## Modelo de segurança (resumo)

- **Segredos:** ficam no Keychain do sistema (MCP, Gemini) e nunca vão para SQLite, logs, relatórios de falha ou para a interface. Tudo o que é persistido passa por redação de padrões de credenciais.
- **Menor privilégio:**
  - agentes rodam em somente leitura por padrão;
  - "acesso total" exige confirmação a cada execução e nunca é usado por automações;
  - capacidades mínimas do Tauri;
  - comandos de shell de rotinas só rodam com aprovação do texto exato (sha256).
- **Isolamento:**
  - worktrees por tarefa;
  - perfil isolado do navegador, com o DevTools só em `127.0.0.1`;
  - memória por workspace (nada é compartilhado entre projetos sem autorização);
  - arquivos de entrada apenas via diálogo nativo.
- **Conteúdo externo** (páginas, imagens, resultados de ferramentas, sugestões de agentes) é tratado como dado não confiável. Nada entra na memória sem revisão humana.
- **Atualizações:** pacotes assinados (minisign) e verificados antes da instalação.
- **Auditoria:** ações sensíveis ficam em Observabilidade → Auditoria.
