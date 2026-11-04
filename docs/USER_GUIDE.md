# FORGE — Guia do usuário

FORGE é um ambiente de desenvolvimento agêntico para desktop (macOS). Ele usa as ferramentas que já estão no seu computador (git, Node, Claude Code, Codex, Gemini CLI, Chrome) e nunca simula integrações: o que não está configurado aparece como indisponível.

## Instalação (macOS)

1. Baixe o `.dmg` da [última release](https://github.com/matheusdevai/forge-ade/releases/latest) para o seu Mac:
   - `aarch64` para Apple Silicon;
   - `x64` para Intel.
2. Abra o `.dmg` e arraste o **FORGE** para **Aplicativos**.
3. **Primeira abertura.** O FORGE ainda não é notarizado pela Apple, então o macOS bloqueia a primeira abertura. Para liberar:
   - clique com o botão direito no app → **Abrir** → **Abrir**; ou
   - abra **Ajustes do Sistema → Privacidade e Segurança** e clique em **Abrir mesmo assim**.
4. As próximas versões são instaladas pelo próprio app em **Configurações → Sobre → Verificar atualizações**, com verificação da assinatura do pacote.

## Primeiros passos

- **Onboarding:** confere git, Node, Rust, Claude Code, Codex, Gemini CLI e Docker, e mostra como instalar o que faltar.
- **Abrir um projeto:** use ⌘O. Cada workspace tem sessões com painéis (terminal, editor, git, agente, navegador…) que você organiza arrastando.
- **Paleta de comandos:** ⌘K mostra todas as ações.

## Atalhos

| Atalho | Ação |
|---|---|
| ⌘K | Paleta de comandos |
| ⌘P | Abrir arquivo |
| ⌘⇧F | Buscar no workspace |
| ⌘⇧G | Git |
| ⌘⇧B | Navegador |
| ⌘I | Painel do agente |
| ⌘J | Terminal |
| ⌘T | Nova sessão |
| ⌘⌥S | Salvar todos |
| ⌘N | Nova missão |
| ⌘O | Abrir workspace |
| ⌘1 – ⌘9, ⌘0 | Dashboard, Missões, Agentes, Providers, Biblioteca, Skills, MCP, Memória, Mídia, Autopilot |
| ⌘⇧J | Jarvis (comando de voz) |
| ⌘⇧D | Ditar para o agente em foco |

## Recursos

- **Agentes:** perfis com provider, modelo, instruções, skills, MCP e permissões.
  - As permissões são somente leitura, pode editar, ou acesso total (que exige confirmação a cada execução).
  - O **Harness** (varinha no painel do agente) sugere a configuração adequada para cada pedido e explica por quê.
- **Missões e Maestro:** um planejador cria tarefas com dependências, e cada tarefa roda numa worktree isolada.
  - Há revisão humana ou de um agente revisor.
  - Nada é integrado ao seu branch sem sua aprovação.
- **Squads e Receitas:** equipes e roteiros reutilizáveis, que podem ser importados e exportados (`.forge.json`).
- **Skills:** instruções versionadas no formato SKILL.md, associadas a agentes, squads ou missões.
- **MCP:** servidores stdio ou HTTP. Os segredos ficam no Keychain e os servidores se anexam por execução (no Claude Code).
- **Navegador:** usa o Chrome instalado, em modo invisível, com perfil isolado.
  - Mostra o console, erros de rede e capturas de tela.
  - **Verificar página** gera um relatório que você pode enviar ao agente para corrigir.
- **Memória:** decisões e convenções por escopo (global, workspace, missão, agente).
  - Nada é compartilhado entre projetos sem sua autorização.
  - Sugestões dos agentes só valem depois que você aprova.
- **Voz:** Jarvis (comandos) e ditado, com o reconhecimento de fala do macOS. Ações sensíveis pedem confirmação.
- **Mídia:** imagens, voz, transcrição e vídeo com Google Gemini (sua chave fica no Keychain), análise de imagem com o Claude Code e transcrição local.
- **Autopilot:** rotinas agendadas com limites diários, tempo máximo e desativação após falhas seguidas.
  - Comandos de shell exigem que você aprove o texto exato.
  - **As rotinas só rodam com o FORGE aberto.**
- **Observabilidade:** métricas, processos, histórico pesquisável com reprodução de execuções, e auditoria.
  - Custos e tokens aparecem só quando o CLI os informa, e nada é estimado.

## Privacidade e segurança

- Chaves e segredos ficam no **Keychain do macOS**, nunca no banco de dados nem em logs.
- A entrada e a saída dos terminais não são gravadas.
- Conteúdo de páginas, imagens e respostas de ferramentas é tratado como **não confiável** (proteção contra prompt injection).
- Relatórios de falha ficam só no seu Mac (**Configurações → Sobre**) e nunca são enviados.

## Onde ficam os dados

`~/Library/Application Support/dev.forge.desktop/`:

- `forge.db` (SQLite);
- `worktrees/`;
- `media/`;
- `browser/` (perfil isolado e capturas);
- `crash/`.
