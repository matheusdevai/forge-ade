# Arquitetura

```
forge/
├── src/                      Frontend (React 19 + TS + Vite + Tailwind v4 + Zustand)
│   ├── brand/                Logo/mascote oficiais (slots + estados)
│   ├── components/ui/        Primitivos estilo shadcn (Radix + cva)
│   ├── components/layout/    Sidebar, TitleBar, CommandPalette
│   ├── features/
│   │   ├── dashboard/
│   │   ├── workspace/        WorkspaceView + ações (abrir/registrar)
│   │   ├── panes/            Pane Engine: layout.ts (modelo puro), store, grid, registry, kinds/
│   │   ├── missions/         Rascunhos de missão (engine no M08)
│   │   ├── environment/      Tabela de detecção de ferramentas
│   │   ├── settings/
│   │   └── onboarding/
│   ├── stores/               Zustand: app (rotas/UI), settings, data (workspaces, missões, atividade, ambiente)
│   └── lib/ipc.ts            Única ponte tipada para os comandos Rust
├── src-tauri/                Core desktop (Tauri 2 + Rust)
│   ├── migrations/           SQL versionado (PRAGMA user_version)
│   └── src/
│       ├── db/               Conexão, migrations e repositories (funções sobre &Connection)
│       ├── environment.rs    Detecção real de CLIs (PATH do login shell + timeout)
│       ├── workspace_fs.rs   Leitura de arquivos confinada ao workspace
│       ├── terminal.rs       Sessões PTY (portable-pty): spawn, stream, resize, kill
│       ├── search.rs         Lista de arquivos e busca de texto (crate ignore, respeita .gitignore)
│       ├── git.rs            Git via binário do usuário: status, stage parcial, commit, branches, stash, remotos, worktrees
│       ├── providers/        Adaptadores de agentes (Claude Code, Codex, Gemini): status, login, headless, verificação
│       ├── agents/           Agent Engine: perfis, runner sem interface, normalização de streams, execuções e eventos
│       ├── missions/         Mission Engine: plano (tarefas e DAG), execução em worktrees, revisão e integração
│       ├── library/          Squads, receitas e bundles (.forge.json) de importação e exportação
│       ├── skills/           Skills SKILL.md: parser, varredura segura, versões, associações e composição
│       ├── mcp/              Servidores MCP: cliente (stdio/HTTP), cofre de segredos, modelos, associações, anexação ao Claude
│       ├── redact.rs         Mascaramento de segredos para tudo que é persistido ou exibido
│       ├── commands.rs       Superfície IPC fina (valida → repository → log de atividade)
│       └── error.rs          AppError serializado como { kind, message }
├── assets/brand/             Arquivos oficiais da marca
└── docs/
```

## Fluxo de dados

`Componente → store (Zustand) → lib/ipc.ts → comando Tauri → repository (SQLite)`

- Stores nunca acessam `invoke` diretamente; toda chamada passa por `api.*` em `lib/ipc.ts`.
- Repositories são funções puras sobre `&rusqlite::Connection`, testadas com banco em memória.
- `Db` serializa o acesso com `Mutex<Connection>`; WAL + `foreign_keys=ON`.

## Pane Engine

- **Modelo** (`features/panes/layout.ts`): árvore serializável (`pane` | `split` com `sizes` em %) e
  uma bandeja `minimized` (pane + vizinho-âncora + lado). Todas as operações são funções imutáveis:
  split, add, close (colapsa splits de um filho), duplicate, **movePane** (encaixe nas 4 bordas ou troca
  no centro), **minimize/restore** (volta ao lado do vizinho original; sem âncora, vai para a borda direita),
  maximize, cycle focus, `toTemplate`/`instantiate` (layouts reutilizáveis com ids novos) e `parseLayout`
  (validação completa ao carregar, ids únicos, reparo de referências quebradas).
- **Sessões** (`store.ts`): cada workspace tem de 1 a 20 sessões (abas), cada uma com seu `LayoutDoc`, na tabela
  `workspace_sessions`. A sessão ativa fica em `workspaces.active_session_id`. Cada mudança é persistida por
  sessão com debounce de 350 ms e um flush ao trocar de workspace ou fechar a janela.
- **Layouts salvos**: os embutidos ficam em `presets.ts` e os do usuário em `layout_presets` (globais, nome único sem
  diferenciar maiúsculas). Aplicar um layout substitui o da sessão atual e oferece "Desfazer".
- **Arrastar e soltar** (`dnd.ts`): eventos de ponteiro (não HTML5 DnD, que o WebView2 intercepta no Windows),
  limiar de 4 px, hit-test por `[data-pane-id]`, zona calculada por `dropZoneFor` e cancelamento com Esc ou perda de foco.
- **Render**: `react-resizable-panels` v4. Tipos de pane vêm de `registry.tsx` e um tipo só fica disponível
  quando tem componente implementado.

## Terminal (M03)

```
xterm.js ──onData──▶ terminal_write ──▶ PTY writer ──▶ shell
xterm.js ◀─Channel── TermEvent{data|exit} ◀── pump thread ◀── PTY reader
```

- `TerminalManager` (Rust, estado global do app) guarda as sessões: master PTY, writer, killer,
  scrollback em anel (512 KB, corte em fronteira de caractere e de linha) e **um** assinante.
- Uma thread por sessão lê o PTY, decodifica UTF-8 em streaming (sequências partidas entre leituras
  são preservadas), anexa ao scrollback e emite para o assinante sob o mesmo lock. Assim, `attach`
  devolve o snapshot e passa a receber ao vivo sem perder nem duplicar bytes.
- `attach` retorna um token, e `detach(token)` só remove o assinante se ainda for o dono, o que resolve corridas de remontagem.
- O shell é o `$SHELL` do usuário como login shell (`-l`), ou `COMSPEC` no Windows, com o PATH completo resolvido
  em `environment.rs`, `TERM=xterm-256color` e `TERM_PROGRAM=FORGE`. O diretório inicial é sempre a raiz do workspace
  (ou um subdiretório validado por `workspace_fs::resolve`).
- **Ciclo de vida**: o terminal sobrevive à troca de aba, sessão ou workspace e ao minimizar o pane. Fechar o pane
  mata o processo na hora. Terminais que nenhum pane referencia (sessão fechada, tipo trocado, layout
  aplicado) são coletados após 8 s, se tiverem mais de 15 s de vida, o que dá tempo para "Desfazer". Remover o workspace
  e sair do app (`RunEvent::Exit`) encerram tudo.
- O `terminalId` é estado efêmero (`EPHEMERAL_STATE_KEYS`): cópias de pane ou sessão nunca herdam o processo.
- Limites: 32 terminais vivos e 64 KB por escrita (o frontend fatia colagens em blocos de 32 KB).
- O pane é carregado sob demanda (`React.lazy`), porque o xterm só entra no bundle quando é usado.

## Editor (M04)

- **Monaco** (`features/editor/`) é carregado sob demanda, com workers locais (`?worker` do Vite, sem CDN), tema `forge-dark`
  e diagnósticos semânticos de TS/JS desligados (os arquivos são abertos sem um projeto TS completo; a sintaxe continua validada).
- **Modelos compartilhados** (`models.ts`): um buffer por arquivo, compartilhado entre panes. Ele guarda a versão salva
  (`alternativeVersionId`), o `mtime` do disco e o motivo de somente leitura (arquivo maior que 5 MB). O estado de alterações e conflitos fica em
  `status.ts`, sem dependência do Monaco, para a árvore, as abas e a proteção ao fechar a janela.
- **Salvar**: `fs_write_text` grava num temporário no mesmo diretório, faz fsync, preserva as permissões e só então
  renomeia sobre o original (o rename atômico). Ele recebe o `expected_mtime_ms`: se o arquivo mudou fora do FORGE, retorna
  `conflict` e o editor oferece **Recarregar do disco** ou **Sobrescrever**. Ao focar a janela e ao ativar uma aba, um
  `fs_stat` detecta mudanças externas: buffers limpos recarregam sozinhos e os sujos são marcados em conflito.
- **Abrir arquivos** (`actions.ts`, sem Monaco): reaproveita o editor em foco ou o último usado, ou restaura um
  minimizado, ou cria um ao lado. Revelar uma linha vem da busca. Renomear e excluir propagam para as abas de **todas** as sessões
  (`applyAll`) e para os buffers, mantendo as edições não salvas.
- **Fechar com alterações**: a aba pede Salvar, Descartar ou Cancelar, e a janela usa `onCloseRequested` com
  Salvar tudo e fechar, Fechar sem salvar ou Cancelar.
- **Busca** (`search.rs`): usa o mesmo walker do ripgrep (`ignore`). Respeita `.gitignore`, `.ignore` e as exclusões globais do git, e sempre
  ignora `.git/`, `node_modules/` e binários, além de arquivos com mais de 2 MB. Aceita regex, maiúsculas e palavra inteira, com colunas em UTF-16
  (compatíveis com Monaco e JS) e no máximo 2.000 resultados. O Quick Open usa `fs_list_files` (até 50 mil arquivos) com um matcher fuzzy local.
- **Explorer**: criar (sem sobrescrever), renomear (sem sobrescrever e sem mover uma pasta para dentro de si mesma), copiar o caminho
  e **mover para a lixeira do SO** (crate `trash`, recuperável). Nada é apagado de forma permanente.

## Git (M05)

- **Execução**: `git.rs` chama o `git` do usuário diretamente (sem shell), sempre com `--` antes de caminhos, contra o top-level
  do repositório (`rev-parse --show-toplevel`). O `prefix` mapeia caminhos do repo para caminhos do workspace (que pode ser uma subpasta).
- **Nunca interativo**: `GIT_TERMINAL_PROMPT=0`, SSH em `BatchMode`, `GIT_EDITOR=true` e `GCM_INTERACTIVE=never`. A autenticação usa
  a configuração existente do usuário (keychain, credential helper, agente SSH). O FORGE não vê nem guarda credenciais, e o stderr
  passa por `redact` (`https://user:token@host` → `https://***@host`).
- **Validação**: caminhos sem `..` e não absolutos, refs com conjunto restrito de caracteres e nunca começando com `-` (o que evita injeção
  de opções), e nomes de branch via `git check-ref-format`.
- **Status**: `status --porcelain=v2 -z` (até 5 mil arquivos) mais `MERGE_HEAD` e rebase. O pane atualiza ao montar, ao focar a janela,
  ao salvar arquivos (evento `forge:files-changed`), após operações (`forge:git-changed`) e a cada 10 s enquanto está visível.
- **Diff**: o conteúdo de cada lado vem de `git cat-file blob` (HEAD, `:path` para o índice, `<sha>^`) ou do disco, e é
  renderizado no diff editor do Monaco. Binários e arquivos acima de 5 MB não são comparados.
- **Stage por linhas**: `lineChanges.ts` recombina o conteúdo a partir das `ILineChange` do Monaco e da seleção. O resultado vai
  para o índice com `hash-object -w --stdin` + `update-index --cacheinfo` (preservando o modo), sem tocar na árvore de trabalho.
  Descartar uma seleção grava a árvore de trabalho pelo `fs_write_text` atômico, com checagem de mtime.
- **Operações seguras por padrão**: pull só fast-forward, push nunca forçado (sem upstream, publica com `-u` no `origin`
  ou no único remoto), excluir branch não integrada exige uma segunda confirmação, descartar arquivo novo manda para a lixeira,
  e não existe `reset --hard` nem `push --force` na interface.
- **Worktrees**: criadas em `<app data>/worktrees/<repo>-<hash>/<branch>`, fora do repositório. Podem ser registradas como
  workspace ("Repo ⎇ branch"). Ao remover, encerra os terminais e tira o workspace da lista. A worktree principal é protegida, e
  uma worktree com alterações só é removida com confirmação extra (`--force`). Esta é a base para dar a cada tarefa de agente
  sua própria branch e pasta (M08–M09).
- Depois de trocar de branch, fazer pull, stash, merge ou descartar, os buffers abertos no editor são conferidos com o disco (`checkAllOpen`).

## Providers (M06)

- **Registro** (`providers/mod.rs`): uma definição por CLI (binário, comando de instalação, fluxo de login, flag de modelo,
  aliases documentados pelo próprio CLI e capacidades). Os adaptadores cobrem detecção de versão (via `environment`), **estado de login**,
  modelo configurado e execução sem interface.
- **Login nativo, sem credenciais no FORGE**:
  - Claude Code: `claude auth status --json`. Só são mantidos `loggedIn`, método e plano; e-mail e IDs são descartados.
  - Codex: `codex login status` (texto e código de saída).
  - Gemini (sem comando de status): o `selectedType` de `~/.gemini/settings.json` mais a **presença** de `oauth_creds.json`, `.env` ou
    `GEMINI_API_KEY`/`GOOGLE_API_KEY`. Nunca o conteúdo.
- **Ambiente**: os agentes recebem o ambiente do *login shell interativo* do usuário (`environment::login_env`, capturado uma
  vez com `$SHELL -ilc 'env -0'`), para que chaves e configurações do `.zshrc` funcionem como no terminal. Esse ambiente fica só em memória
  e nunca é logado, persistido ou enviado ao frontend.
- **Panes de agente** (`claude`, `codex`, `gemini`): o CLI interativo roda direto no PTY do M03 (sem shell intermediário), no diretório
  do workspace. O modelo é opcional e validado (nunca vira uma flag). Há um modo "login", que roda o fluxo de login do próprio CLI no pane. As mudanças de
  modo ou modelo reiniciam a sessão (`launchNonce`). Os processos entram na mesma coleta de órfãos dos terminais.
- **Sem interface** (base para o M07): Claude `-p --output-format stream-json --verbose --permission-mode plan`,
  Codex `exec --json --sandbox read-only` e Gemini `-p -o json --approval-mode plan`. Os parsers são tolerantes a linhas não-JSON
  (banners do shell). O erro do Gemini chega como JSON no stderr.
- **Verificação real** (`provider_verify`): um prompt mínimo deve voltar com `FORGE_OK`. O resultado (versão, modelo, duração, tokens e custo,
  **apenas quando informados pelo CLI**) fica em `provider_checks`. "Verificado" significa um teste OK **na versão instalada**: ao atualizar o
  CLI, ele volta a ser "não verificado".
- **Métricas**: o Codex e o Gemini não informam custo, então o FORGE não estima. O custo do Claude Code é o valor equivalente de API reportado
  pelo CLI e é rotulado assim (não é uma cobrança à parte em planos de assinatura).

## Agent Engine (M07)

```
AgentRunPane ──agent_run_start──▶ RunManager ──spawn (grupo de processos)──▶ CLI -p/exec --json
     ▲                                  │ stdout linha a linha
     └──Channel<RunMessage>── LiveRun ◀─┤ StreamParser (por provider) → AgentEvent normalizado
                                        └─▶ SQLite (agent_events, redigidos) + agent_runs (resumo)
```

- **Perfis** (`agent_profiles`): identidade, função, provider, modelo, instruções, permissão padrão e limite de turnos. Os 10 perfis
  da especificação são criados na inicialização (sem sobrescrever edições) e podem ser restaurados. Skills e memória por agente vêm no M11 e no M14.
- **Execução** (`runner.rs`): valida o prompt e a permissão (acesso total exige `confirmFull`) e respeita o limite de 8 execuções simultâneas e o timeout de 30 min.
  Roda no diretório do workspace ou numa **worktree nova** (`forge/agent-<id>`), registra o `base_commit` e inicia o CLI em seu próprio
  grupo de processos (cancelar envia TERM e depois KILL ao grupo). O prompt do Codex vai por stdin (`-`), para nunca ser lido como flag.
- **Permissões → flags nativas**: leitura (`plan` / `read-only` / `plan`), edição (`acceptEdits` / `workspace-write` / `auto_edit`) e total
  (`bypassPermissions` / `danger-full-access` / `yolo`). As instruções vão por `--append-system-prompt` no Claude e como prefixo nos demais.
- **Normalização** (`stream.rs`): `session`, `text_delta` (só ao vivo), `text`, `tool_use`, `tool_result`, `warning`, `error` e `result`
  (tokens e custo só quando informados). Tudo passa por `redact::sanitize` antes de sair do parser.
- **Attach**: snapshot do banco mais assinatura ao vivo sob o mesmo lock da persistência (sem perder nem duplicar eventos), com token para detach.
  Execuções finalizadas são lidas do banco. Execuções que ficaram "running" quando o app fechou são marcadas como interrompidas ao iniciar.
- **Continuação**: `parentRunId` retoma a sessão nativa (Claude `--resume`, Codex `exec resume`) no mesmo diretório ou worktree.
- **Alterações**: `agent_run_changes` mostra o `git status` do diretório da execução e os commits feitos desde o `base_commit`.

## Mission Engine (M08)

```
draft ──plan──▶ planned ──start──▶ running ⇄ paused ──▶ completed ──integrate──▶ (merge na branch base)
tarefa: pending ─▶ running ─▶ review ─▶ approved          (rejected / failed / cancelled ─▶ retry | skip)
```

- **Planejamento**: o perfil Software Architect roda em **somente leitura** no workspace e responde um JSON entre `<forge-plan>…</forge-plan>`.
  `tasks::parse_plan` tolera texto e cercas ao redor, gera chaves ausentes, mapeia papéis para perfis (com fallback) e valida o DAG
  (Kahn: sem ciclos, sem dependências inexistentes, ordem topológica estável). O plano pode ser editado ou montado à mão (`PlanEditor`).
- **Execução** (`engine.rs`): ao iniciar, grava `base_commit` e `base_branch`. O `advance` lança as tarefas cujas dependências foram aprovadas
  ou puladas, até o limite `max_parallel` (1 a 4). Cada tarefa ganha uma worktree `forge/mission-<id>/<key>-<tentativa>` criada a partir da branch da
  1ª dependência (ou da base), com merge das demais, e roda o perfil dela (o acesso total é rebaixado para edição em missões).
- **Término** (callback do runner): resultado concluído recebe commit automático na branch da tarefa (com os hooks do usuário) e vai para
  `review` (ou `approved` sem revisão). Falha ou cancelamento ficam registrados. Depois, `advance` de novo.
- **Revisão**: aprovar libera os dependentes, rejeitar registra o feedback, refazer descarta a worktree e roda de novo com o feedback no prompt,
  e pular desbloqueia os dependentes.
- **Controles**: pausar (as tarefas em execução terminam, nenhuma nova começa), retomar e cancelar (interrompe os agentes e cancela as pendentes).
- **Integração**: exige o workspace sem alterações rastreadas e sem merge em andamento. Faz merge das branches aprovadas na ordem do plano e para no
  1º conflito (resolvido no Git Center, depois é só integrar de novo; tarefas já integradas são puladas). "Limpar worktrees" remove as pastas e mantém as branches.
- **Visibilidade**: `mission_events` (histórico com segredos mascarados), `mission_runs` (todas as execuções da missão) e totais de tokens,
  tempo e custo informado. A UI recebe `forge://mission-changed` a cada transição.

## Maestro (M09)

- **Modos** (por missão, alteráveis até a conclusão):
  - **Free**: a revisão humana é opcional (`require_review`).
  - **Squad**: ao concluir uma tarefa, o Maestro inicia o **Code Reviewer** (somente leitura) na worktree dela. O veredito
    (`<forge-review>{"verdict","summary","issues"}`) vira uma recomendação na tarefa e o humano decide.
  - **Agentic**: o veredito é aplicado sozinho. Aprovar libera os dependentes. Rejeitar ou falhar segue `maestro::decide_on_failure`: refazer com o feedback
    até `max_attempts`, depois reatribuir **uma vez** a outro provider instalado e logado (`provider_override`, sondado só nesse momento),
    depois escalar para o humano (evento `blocked`).
- **Limites**: `max_runs` (todas as execuções da missão: plano, tarefas e revisões) e `budget_usd` (soma dos custos **informados**). Ao atingir um deles,
  o Maestro pausa a missão com o motivo. Paralelismo de 1 a 4.
- **Gates humanos fixos**: aprovar o plano, integrar na branch e acesso total (em missões, o acesso total é rebaixado para edição).
- **Base de revisão**: cada tarefa grava o `base_commit` da worktree no início. O revisor usa `git diff base..HEAD`, e o relatório lista
  `git log base..branch`.
- **Relatório final** (`maestro::build_report`, determinístico): modo, base, totais, briefing, por tarefa (agente, provider e eventual
  reatribuição, tentativas, branch, commits, revisão do Maestro) e pendências e riscos (tarefas não aprovadas, aprovações contra a recomendação do Maestro).
  É gerado antes de a missão ficar "completed" e regenerado após a integração.

## Squads & Receitas (M10)

- **Squads** (`library/squads.rs`): membros `{profileId, function, stage}`, fluxo, regras e modo padrão. São 5 padrão (APP FACTORY,
  CINEMA SITE, SAAS FACTORY, AUTOPILOT, ARCADE). Missões com squad passam pelo `MissionEngine::planning_profiles`: o planejador é o membro
  planner, architect ou pm; o prompt recebe `squads::planning_context` (membros, etapas, fluxo e regras); `parse_plan` resolve papéis só entre os membros; e cada
  tarefa recebe as regras do squad no prompt.
- **Receitas** (`library/recipes.rs`): campos tipados com placeholders validados, squad, modo e limites do Maestro, plano opcional (com
  placeholders). `instantiate` cria a missão numa transação (briefing renderizado, configurações e, com plano pronto, tarefas e status `planned`).
  `from_mission` gera uma receita a partir de uma missão. Há 8 receitas padrão, 3 delas com plano pronto (Refatorar, Corrigir bugs, Auditar segurança).
- **Bundles** (`library/bundle.rs`): `{format:"forge.bundle", version:1, kind, name, profiles[], squad?, recipe?}`. Na exportação, só os perfis
  não padrão são embutidos. A importação passa por parse estrito, prévia (provider conhecido, referências resolvidas, `full → edit`, avisos), confirmação
  do usuário e criação em uma transação com novos IDs, nomes sem colisão ("(importado)") e `origin = import`.

## Skills (M11)

- **Formato** (`skills/format.rs`): `SKILL.md` com frontmatter (`name`, `description`; outras chaves são ignoradas) e corpo em Markdown. O slug vem do nome
  (sem acentos). Limites: corpo de até 64 KB, até 50 arquivos de apoio e 2 MB no total. Links simbólicos, `.ocultos` e `node_modules` são ignorados, e scripts são marcados.
  `risk_warnings` sinaliza instruções suspeitas na prévia.
- **Repositório** (`skills/repo.rs`): `skills` (conteúdo atual), `skill_versions` (toda alteração), `skill_files` (só armazenamento) e `skill_links`
  (agente, squad ou missão). Inclui as 7 skills padrão da especificação (React, Next.js, Supabase, SaaS Architect, Security Auditor, UI Designer, Testing Expert), sem associação
  por padrão (nenhum custo de contexto até o usuário decidir).
- **Aplicação**: `compose(profile, squad, mission)` monta o bloco `<skills>` com as skills ativas, deduplicadas e dentro do limite de 48 KB, e `with_skills` o anexa às
  instruções (`--append-system-prompt` no Claude, prefixo do prompt no Codex e no Gemini). Isso vale no pane Agente (skills do perfil) e nas missões (planejador,
  tarefas e revisor recebem as do agente, do squad e da missão). Cada execução grava `skills: ["slug@vN"]`.
- **Instalação**: `skill_pick` (diálogo nativo no Rust, varredura e prévia com token), depois `skill_install(token, update)`. Uma skill de mesmo nome só é atualizada
  com confirmação, como nova versão. Skills padrão não são sobrescritas por instalação. A exportação gera `<slug>/SKILL.md` mais os arquivos de texto, e os caminhos
  são validados para nunca escreverem fora da pasta escolhida.

## MCP (M12)

- **Registro** (`mcp/repo.rs`): servidores stdio (comando, args, env) ou HTTP (URL, cabeçalhos), com `secret_keys` declarados, somente leitura, ativo e
  modelo de origem. A validação cobre nome de variável, cabeçalho, `https` (http só em localhost) e placeholders `${CHAVE}`, que precisam ser segredos declarados.
- **Segredos** (`mcp/secrets.rs`): a trait `SecretStore`, com `KeychainStore` em produção e `MemoryStore` nos testes. A conta é `<server_id>:<CHAVE>` no serviço
  `dev.forge.desktop.mcp`. Remover o servidor ou deixar de declarar uma chave apaga o valor do cofre. O status ("definido") só é lido no detalhe do servidor,
  porque ler o Keychain pode pedir autorização.
- **Verificação** (`mcp/client.rs`): `initialize` (protocolo `2025-06-18`), `notifications/initialized` e `tools/list` paginado. Em stdio, recusa requisições
  servidor→cliente, ignora notificações e roda em grupo de processos encerrado por inteiro. Em HTTP, aceita JSON ou SSE, guarda o `Mcp-Session-Id` e encerra a sessão com DELETE.
  O log e os erros passam pela remoção dos valores secretos e por `redact`.
- **Anexação**: `prepare_run(profile ∪ squad ∪ mission)` gera, no Claude, um arquivo `mcp-run/<id>.json` (0600) com `{"mcpServers": …}`, que o runner passa via
  `--mcp-config` (com `--strict-mcp-config` e `--allowedTools`) e apaga ao fim da execução. A execução registra `mcp: [nomes]`. Nos outros providers, o FORGE registra uma nota
  ("ainda não suportado") e não anexa nada.
- **Modelos**: Filesystem, GitHub (remoto oficial com Bearer), Browser (Playwright), Supabase (somente leitura), PostgreSQL (somente leitura), Docker (gateway do
  MCP Toolkit), Vercel (OAuth, ainda não suportado) e Cloudflare Docs (remoto público). Entradas secretas vão direto para o cofre.

## Navegador (M13)

- **Chrome** (`browser/chrome.rs`): localiza Chrome, Chromium, Brave ou Edge nos caminhos padrão e inicia em `--headless=new` com `--remote-debugging-port=0` em 127.0.0.1 e
  `--user-data-dir=<dados>/browser/profile`. O endpoint vem do `DevToolsActivePort`. O processo roda em grupo próprio.
- **CDP** (`browser/cdp.rs`): um WebSocket síncrono com uma thread que esvazia a fila de saída e lê com timeout curto. `call` espera a resposta pelo `id`; `notify` não espera
  (é usado dentro dos handlers, como o ack de frames). Se a conexão cai, as chamadas pendentes falham e o handler recebe `Forge.disconnected`.
- **Abas** (`browser/manager.rs`): `Target.createTarget` + `attachToTarget{flatten}`. Os eventos são roteados por `sessionId` (aba) ou `targetId` (título e URL).
  O estado de cada aba guarda URL, título, carregamento, viewport, console (anel de 1000), mapa requestId→URL e um assinante (Tauri Channel com token, como no terminal).
  `BrowserEvent` = `frame | nav | console | closed`.
- **Verificar**: marca o último `seq`, recarrega sem cache, espera `loads` aumentar (até 30 s) e mais 1,5 s, então junta as entradas novas, captura a tela e gera o resumo redigido.
- **Prévia** (`browser/preview.rs`): scripts do `package.json` mais o gerenciador, executados via `TerminalManager` sem shell. A URL é detectada no frontend (`extractLocalUrl`).
- **Frontend** (`features/browser`): `BrowserSurface` (barra, tela com `<img>` alimentada pelos frames, `textarea` invisível que recebe o teclado e o colar, console e relatório),
  `BrowserPane` e `PreviewPane` (= `DevServerBar` + `BrowserSurface`). As coordenadas passam por `mapPoint`, que desconta o letterbox do `object-contain`.

## Memória (M14)

- **Repositório** (`memory/mod.rs`): `memories(scope, workspace_id, mission_id, profile_id, kind, title, body, tags, pinned, status, source, source_run_id)`,
  com CHECK de consistência entre escopo e alvo. Os status são `active`, `suggested` e `archived`.
- **Injeção**: em `RunManager::start_with_program`, `compose(workspace, mission, profile)` → bloco `<memory>` (24 KB) acrescentado às instruções,
  mais a dica de como sugerir `<forge-memory>`. Os rótulos ficam em `agent_runs.memory`. O `mission_id` vem do Mission Engine via `RunRequest` (campo não serializado).
- **Sugestões**: ao fim de uma execução concluída, `record_suggestions` lê o `result_text`, cria itens `suggested` (escopo missão ou workspace) e registra a atividade.
- **Frontend** (`features/memory`): `MemoryView` (abas, filtros, cartões, editor com consentimento entre projetos, painel "O que um agente recebe" via `memory_preview`).

## Voz (M15)

- **Helper** (`src-tauri/voice/main.swift` → `binaries/forge-voice-<target>` via `build.rs`, sidecar `Contents/MacOS/forge-voice`).
  Subcomandos: `status`, `authorize`, `locales`, `listen` e `file`. Fala JSON por linha no stdout e lê `stop` no stdin.
- **Rust** (`voice/mod.rs`): `VoiceManager` (uma escuta por vez, eventos via Tauri Channel, `stop` e `cancel`, `say` com texto via stdin, autoteste)
  e `pause_plan` (missões em execução e execuções avulsas). Comandos `voice_*` e `agents_pause_all(dry_run)`.
- **Frontend** (`features/voice`): `intents.ts` (interpretador puro), `store.ts` (escuta, Jarvis e ditado, execução das intenções), `VoiceHud`
  (transcrição ao vivo, resultado e confirmação) e `VoiceSettings`.

## Harness (M16)

- **Núcleo puro** (`harness/mod.rs`): `classify(text)`, `role_for`, `claude_tier(c, pref)`, `turns_for` e `recommend(HarnessInput)` → `Recommendation`
  (com motivos e avisos). `usage(conn, since)` agrega `agent_runs`.
- **Comando** `harness_recommend(text, preference)`: junta o status real dos providers, os perfis, as skills e os MCPs ativos e o histórico, e chama `recommend`.
- **Aplicação**: o `RunRequest` ganhou `extra_skill_ids` e `extra_mcp_ids`, consumidos por `skills::compose_with` e `mcp::prepare_run_with`.
  O painel Agente guarda o ajuste (`harness`) no estado do pane. No Maestro, `MissionEngine::launch` usa `claude_tier` quando `missions.harness` está ligado.

## Mídia (M17)

- **Cliente Gemini** (`media/gemini.rs`): `list_models`, `generate_image` (Gemini IMAGE ou Imagen `predict`), `understand` (inline), `tts` (PCM → WAV),
  `video_start` / `video_poll` / `download`. Os erros passam por remoção da chave e por `redact`.
- **Motor** (`media/mod.rs`): `MediaEngine` (arquivos em `<dados>/media`, estado do provider em `media_providers`, itens em `media_assets` running → done/failed),
  `sniff` por conteúdo, `candidates` e `default_model` por operação, `claude_analyze` (runner args + `StreamParser`) e transcrição via `VoiceManager`.
- **Comandos** (`commands/media.rs`): `media_*`, entrada via `MediaInput` (token do diálogo ou id de item) e polling do vídeo numa thread com evento `forge://media-changed`.
  `browser_screenshot` também registra a captura na biblioteca.
- **Frontend** (`features/media/MediaView.tsx`): abas, provedores e modelos por operação, biblioteca com prévias via Blob URL (`media_read`).

## Autopilot (M18)

- **Núcleo** (`autopilot/mod.rs`): `next_run` (puro, chrono local), repositório `routines` / `routine_runs`, `gate` (sobreposição, limite diário,
  orçamento e aprovação do comando) e `run_process` (grupo de processos, timeout, saída limitada e redigida).
- **Motor** `Autopilot` (managed): `trigger` (execução registrada, ou "pulada" com o motivo), `execute` → `perform` por ação (Agent Engine,
  `preview::command_for`, `$SHELL -lc`, `recipes::instantiate` + `MissionEngine::plan`, `BrowserManager::verify`), contagem de falhas e desativação automática.
  `tick` (agendados e commits) e `start` (fecha órfãs, reagenda e inicia a thread). Os eventos vão via `Notify` → `forge://autopilot-changed` + notificação do SO.
- **Frontend** (`features/autopilot`): `AutopilotView` (lista, editor com aprovação do comando, histórico) e `useAutopilotAlerts`.

## Observabilidade (M19)

- **Núcleo** (`observability/mod.rs`): `metrics(since, workspace, day_starts)` (status, percentis, `Reported<T>` para tokens e custo, por provider e modelo,
  missões, tarefas, rotinas, mídia, ferramentas, série diária), `timeline(TimelineQuery)` (atividade, agent_runs, agent_events tool_use, mission_events,
  routine_runs, media_assets; `audit` restringe a `AUDIT_KINDS` + acesso total), `replay(run_id)` e `ps_stats(pids)`.
- **Comandos** (`commands/observability.rs`): `obs_metrics` (janela em dias locais), `obs_timeline`, `obs_replay` e `obs_processes` (TerminalManager,
  `RunManager::live_pids`, `BrowserManager::pid`, rotinas em andamento, voz e `ps`). As consultas pesadas usam `db::connect` (conexão própria).
- **Frontend** (`features/observability`): `ObservabilityView`, `Overview`, `BarChart` (uma série, dica, tabela), `TimelineList`, `ReplayDialog`, `ProcessTable`
  e os panes `LogsPane` e `MonitorPane`.

## Segurança (M01)

- Capabilities mínimas: `core:default`, arrastar/maximizar janela, `dialog:allow-open`. Sem acesso genérico a FS ou shell pelo frontend.
- Todo acesso a arquivos passa por `workspace_fs::resolve`: só caminhos relativos, canonicalizados e
  contidos na raiz do workspace (bloqueia `..`, caminhos absolutos e fuga por symlink).
- Leitura limitada a 1 MiB e 5000 entradas por diretório; binários não são enviados ao frontend.
- CSP restritiva no `tauri.conf.json`.
- Settings validadas por allowlist de chaves e tipos no Rust.
- Remover um workspace apaga só o registro, nunca os arquivos.
- O log de atividade contém só mensagens curtas, sem conteúdo de arquivos nem secrets.
- Agentes (M07): eventos, prompts, resultados e erros são persistidos **depois** do mascaramento de segredos (chaves de API, tokens do GitHub, Slack, AWS e Google,
  JWTs, chaves privadas PEM, credenciais em URLs e atribuições `*_KEY` / `*TOKEN` / `PASSWORD`), com limite de tamanho por evento.
- Terminal (M03): entrada e saída ficam só em memória, sem nenhum log ou persistência em disco. Links abrem só `http(s)`, `mailto` e `tel`
  (`opener:allow-default-urls`). No Windows e no Linux, atalhos Ctrl não são capturados quando o terminal tem foco.
