# Roadmap

| # | Milestone | Status |
|---|---|---|
| 01 | Foundation | ✅ concluído (2026-11-02) |
| 02 | Panes (drag-and-drop, layouts salvos, múltiplas sessões) | ✅ concluído (2026-11-02) |
| 03 | Terminal (PTY + xterm.js) | ✅ concluído (2026-11-02) |
| 04 | Editor (Monaco) | ✅ concluído (2026-11-02) |
| 05 | Git Center + Worktrees | ✅ concluído (2026-11-03) |
| 06 | Providers (Claude Code, Codex, Gemini) | ✅ concluído (2026-11-03) |
| 07 | Agent Engine | ✅ concluído (2026-11-03) |
| 08 | Mission Engine | ✅ concluído (2026-11-03) |
| 09 | Maestro | ✅ concluído (2026-11-03) |
| 10 | Squads + Recipes | ✅ concluído (2026-11-03) |
| 11 | Skills | ✅ concluído (2026-11-03) |
| 12 | MCP | ✅ concluído (2026-11-03) |
| 13 | Browser | ✅ concluído (2026-11-03) |
| 14 | Memory | ✅ concluído (2026-11-03) |
| 15 | Voice | ✅ implementado (2026-11-03), validação ao vivo do microfone pendente |
| 16 | Harness | ✅ concluído (2026-11-03) |
| 17 | Media | ✅ implementado (2026-11-03), validação ao vivo do Gemini pendente da sua chave |
| 18 | Autopilot | ✅ concluído (2026-11-03) |
| 19 | Observability | ✅ concluído (2026-11-03) |
| 20 | Productization | ✅ concluído (2026-11-03) |

---

## Relatório — Milestone 20: Produtização

### Entregue
- **Atualizações** via GitHub Releases públicas (`matheusdevai/forge-ade`), com `tauri-plugin-updater`.
  - Os pacotes são **assinados com a chave do projeto**: a privada fica em `~/.tauri` (0600), com senha no Keychain; a pública está embutida no app.
  - Configurações → Sobre → **Verificar atualizações** → **Baixar, instalar e reiniciar**. As chamadas passam pelo backend, sem novas permissões na interface.
- **Instância única**: abrir de novo foca a janela existente (sem duas instâncias disputando o banco, o Chrome e o agendador).
- **Relatórios de falha locais**: panics do núcleo (com backtrace), erros da interface (error boundary com tela de recuperação, `window.onerror`, promises rejeitadas), segredos redigidos, limite de 50, e **nada é enviado**. Visíveis em Configurações → Sobre.
  - A seção Sobre deixou de mostrar "Milestone atual: M03", que estava desatualizado.
- **Build de release**: `.app` (15 MB), **`.dmg` (8,7 MB)** e pacote de atualização `.app.tar.gz` + `.sig`. O bundle tem assinatura ad-hoc completa com o entitlement de microfone (sem conta Apple Developer: primeira abertura via "Abrir").
- **CI e release no GitHub Actions**:
  - `ci.yml`: typecheck, vitest, build, `cargo test`, `clippy -D warnings`, em macOS;
  - `release.yml`: com uma tag `v*`, compila para Apple Silicon e Intel, assina os pacotes de atualização e cria a release em rascunho com o `latest.json`.
- **Documentação**: `docs/USER_GUIDE.md`, `docs/DEVELOPMENT.md`, `docs/RELEASING.md` (incluindo o backup da chave), `SECURITY.md`, `CHANGELOG.md` e o README reescrito.

### Testes
- Rust: 177 testes (+2): relatórios de falha (redação, listagem, limite, sem sair da pasta) e escrita no pânico.
- **Validação do build de release**:
  - **assinatura do pacote de atualização conferida contra a chave pública do `tauri.conf.json`** (a mesma verificação do atualizador);
  - **pacote adulterado recusado**;
  - `codesign --verify --deep --strict` ok (`dev.forge.desktop`, com hardened runtime e entitlement de microfone no app e no helper de voz);
  - `hdiutil verify` do `.dmg` válido;
  - o **app de release abriu** sem nenhum relatório de falha;
  - **instância única** confirmada (mesmo com `open -n`, um único processo).
- Frontend: 67 testes. `tsc` ok e `cargo clippy --all-targets -- -D warnings` sem avisos.
- **Varredura de segredos** antes do primeiro commit público: só fixtures sintéticos de teste, e a chave privada fica fora do repositório.

### Pendências conhecidas
- **Sem notarização Apple**: a primeira abertura exige "Abrir" manual. O caminho para notarizar está documentado.
- O teste ponta a ponta do atualizador (instalar uma v0.1.1 sobre a v0.1.0) só é possível depois de publicar duas versões.
- As rotinas do Autopilot continuam rodando só com o app aberto. O agente de fundo do macOS não foi instalado (exigiria sua autorização).
- Logo e mascote oficiais continuam pendentes.

---

## Relatório — Milestone 19: Observabilidade, Histórico e Auditoria

### Entregue
- **Tela Observabilidade** (sidebar e paleta), com as abas Visão geral, Processos, Histórico e Auditoria.
- **Visão geral**, por período (24 h, 7, 30 ou 90 dias) e por workspace:
  - execuções por status;
  - duração **p50 e p95**;
  - tokens e **custo informado**, sempre com a **cobertura** ("informado em X de Y"). Quando nenhum CLI informou o valor, aparece **"—"**, nunca 0, e **nada é estimado**;
  - missões, tarefas, rotinas (falhas, puladas, desativadas), mídia e ferramentas usadas;
  - **dois gráficos de uma série** (execuções por dia e custo informado por dia, nunca eixo duplo), com dica ao passar o mouse e visão em tabela. Os dias sem custo informado aparecem como "não informado";
  - tabela por provider e modelo.
- **Processos ao vivo**: app, terminais, CLIs interativos, agentes, Chrome headless, rotinas em andamento e microfone aberto, com **CPU e memória reais** (`ps -A`) atualizadas a cada 3 s.
- **Histórico (Redline)**: busca unificada em pedidos, respostas e erros de agentes, **comandos e ferramentas usados pelos agentes** (`Bash: npm test`, `Read: arquivo`), eventos de missão, rotinas, mídia e atividade.
  - Filtros por fonte, nível, workspace e período.
  - **Recuperar contexto**: abrir a missão ou a rotina, ou **reproduzir a execução** evento por evento no ritmo original (1×, 4× ou 16×, pausar, recomeçar).
  - Terminais não são gravados (privacidade), e a tela diz isso.
- **Auditoria**: execuções com **acesso total**, chave do Gemini (salvar e remover), **segredos do MCP** (definir e remover, só o nome da chave), push, descarte, exclusões, merge, worktrees, instalação de skills, importações, rotinas e "pausar todos".
- **Painéis de workspace "Logs" e "Monitoramento"** (reservados desde o M02) agora funcionam: histórico ao vivo do projeto, e processos com métricas das últimas 24 h.
- Todo texto que sai do módulo passa de novo por `redact_secrets`.

### Correção encontrada pelos testes
- Havia uma **corrida no runner**: a execução saía da lista "ao vivo" antes de as sugestões de memória do M14 serem gravadas, e quem esperava o fim podia não vê-las. A gravação agora acontece antes do encerramento (o teste passou 3 de 3 vezes).

### Testes
- Rust: 175 testes (+7):
  - métricas separando valores informados de não informados (Codex sem custo = `None`, nunca 0), percentis e dias;
  - histórico unindo fontes, ordenado, filtrado, sem vazar outro workspace e **com segredo redigido na saída**;
  - busca com `%` e `_` escapados;
  - auditoria só com ações sensíveis;
  - reprodução com os deslocamentos originais;
  - títulos de ferramentas;
  - `ps` real;
  - janelas por dia local.
- **Ao vivo com CLIs reais** (banco temporário: o banco do app ainda está vazio porque você não usou o app no dia a dia):
  - o **Claude Code** leu `hello.txt` com a ferramenta Read e respondeu "OBS-4242", custo informado **US$ 0,111**;
  - o **Codex**, deslogado, falhou com 401 real e aparece com **custo não informado**;
  - o histórico mostrou `Read: …/hello.txt`, e a reprodução teve 5 eventos (sessão → ferramenta → resultado → texto → fim).
- Frontend: 67 testes (+2), incluindo "nunca mostrar dado desconhecido como zero". `tsc` ok e `cargo clippy --all-targets` sem avisos. `FORGE.app` reconstruído.

### Pendências conhecidas
- CPU e memória cobrem o processo principal de cada item, não os subprocessos (por exemplo, os helpers do Chrome).
- Mensagens do console do navegador e entrada e saída dos terminais não são persistidas (privacidade), então não entram no histórico.

---

## Relatório — Milestone 18: Autopilot

### Entregue
- **Rotinas** (tela Autopilot, ⌘0, sidebar e paleta), sempre associadas a um workspace. Ações:
  - **Agente**: perfil + pedido, permissão **leitura ou edição** (acesso total é recusado no backend), worktree opcional. Roda pelo Agent Engine, com skills e memória.
  - **Script** do `package.json` (`<pm> run`, sem shell).
  - **Comando** no shell do usuário, **com aprovação explícita do texto exato** (sha256). Editar o comando invalida a aprovação, e uma alteração direta no banco é detectada na execução.
  - **Missão de receita**: cria (título com data e hora) e inicia o planejamento. Executar continua exigindo aprovação.
  - **Verificar página** no navegador do FORGE: falha se houver erros.
- **Gatilhos**:
  - intervalo (5 min a 7 dias);
  - diário em um horário, com dias da semana (fuso local, horário de verão tratado);
  - **novo commit** (o primeiro HEAD é só a referência);
  - manual ("Executar agora").
- **Controles contra execução infinita ou não autorizada**:
  - sem sobreposição;
  - limite por dia;
  - tempo máximo, encerrando **o grupo de processos inteiro**;
  - **desativação automática após N falhas seguidas**, com motivo visível;
  - orçamento diário contando **só custo informado pelo CLI**;
  - reagendamento antes de executar (sem rajadas);
  - execuções perdidas com o app fechado **não são repetidas**;
  - execuções órfãs fechadas como canceladas ao abrir.
- **Registro**: cada execução guarda gatilho, status, duração, saída (64 KB, com segredos redigidos), erro, agente, missão e custo informado. O Histórico permite cancelar uma execução em andamento.
- **Notificação de falhas**: notificação do macOS (`tauri-plugin-notification`, disparada pelo core), aviso no app com atalho para a tela e log de atividade.
- Migration 0013 (`routines` e `routine_runs`).

### Testes
- Rust: 168 testes (+10):
  - agenda (intervalo, diário, dias úteis, só segundas, horário inválido);
  - captura de processos (código de saída, **timeout matando o grupo** em menos de 5 s, segredo redigido);
  - **validação** (acesso total recusado, comando sem aprovação recusado, edição exige reaprovação, adulteração detectada, intervalo mínimo, script inexistente, `file://`);
  - comando e script reais no diretório do workspace, com eventos;
  - **sem sobreposição, limite diário e desativação automática**, com notificação;
  - orçamento só com custo informado;
  - **o agendador dispara uma vez e reagenda**, e uma rotina desativada nunca dispara;
  - **gatilho de commit com git real**;
  - rotina de agente pelo Agent Engine (CLI falso, permissão `read` e custo registrados);
  - órfãs.
- **Ao vivo com o Claude Code** (haiku, leitura): a rotina respondeu "AUTOPILOT-OK" com **custo informado de US$ 0,0798**.
- Frontend: 65 testes (+2). `tsc` ok e `cargo clippy --all-targets` sem avisos. `FORGE.app` reconstruído, e o banco real migrou para **v13**.

### Pendências conhecidas
- As rotinas só rodam com o FORGE aberto. Um agente de fundo do sistema (launchd) fica para o M20, com sua autorização.
- A notificação do macOS pode pedir permissão na primeira falha, e builds ad-hoc podem pedir de novo.
- O gatilho de commit verifica a cada 15 s (polling), sem hooks do git.

---

## Relatório — Milestone 17: Media Engine

### Entregue
- **Tela Mídia** (⌘9, sidebar e paleta) com Gerar imagem, Analisar imagem, Voz (TTS), Transcrever, Vídeo e **Biblioteca**.
  - Pré-visualização de imagem, áudio e vídeo.
  - "Salvar no workspace": copia sem sobrescrever e só dentro do projeto.
  - Revelar no Finder e excluir com confirmação.
  - Opção de vincular os itens a um workspace.
- **Google Gemini** (API oficial v1beta):
  - a chave fica **no Keychain** e só é enviada **no cabeçalho `x-goog-api-key`**, nunca na URL nem em logs, e é removida de mensagens de erro;
  - **modelos descobertos na sua conta** (`models.list` com paginação), sem nenhum nome fixo no código, com escolha por operação;
  - imagem via `generateContent` (modalidade IMAGE) ou Imagen `:predict`;
  - análise de imagem e transcrição via `generateContent` com o arquivo inline (até 20 MB);
  - TTS com as vozes pré-definidas, convertendo PCM em WAV;
  - vídeo via Veo `predictLongRunning`, acompanhado em segundo plano a cada 10 s por até 15 min, com download só de hosts do Google;
  - bloqueios de segurança explicados com o motivo informado pela API.
  - O provider só fica "verificado" depois de um teste real, e nenhuma operação roda sem verificação.
- **Sem chave**:
  - **análise de imagem pelo Claude Code** (somente leitura, diretório isolado; o texto dentro da imagem é tratado como dado);
  - **transcrição pelo macOS** (helper do M15);
  - **capturas do navegador** entram automaticamente na biblioteca.
- Segurança:
  - a interface nunca envia caminhos arbitrários: arquivos vêm do **diálogo nativo** (token de uso único) ou da biblioteca;
  - o tipo é detectado **pelo conteúdo**, não pela extensão;
  - operações longas usam conexão própria do SQLite (não travam o app).
- Migration 0012 (`media_assets`, `media_providers`). CSP: `media-src 'self' data: blob:`.

### Testes
- Rust: 158 testes (+13). Contra uma **API Gemini falsa local**:
  - paginação de modelos, **chave só no cabeçalho** (verificado em cada requisição) e erros sem eco da chave;
  - imagem via Gemini e via Imagen, bloqueio com motivo;
  - análise e TTS → WAV válido;
  - ciclo do vídeo e recusa de download de host não-Google.
  - Fluxo completo do motor: testar, escolher modelos, gerar, analisar o resultado e listar.
  - Sem provider, **nada é simulado**: operações recusadas e nenhum item criado.
  - Detecção de tipo por conteúdo, WAV, escolha de modelos por operação, salvar no workspace sem sobrescrever nem sair da pasta, e exclusão que não apaga arquivos fora de `media/`.
- **Ao vivo**: o Chrome renderizou "ANVIL 4217", o FORGE capturou a tela e o **Claude Code leu "ANVIL 4217"** na imagem.
- Frontend: 63 testes. `tsc` ok e `cargo clippy --all-targets` sem avisos. `FORGE.app` reconstruído, e o banco real migrou para **v12**.

### Pendências conhecidas
- **O Gemini ainda não foi testado ao vivo**: depende de você colar a chave em Mídia → Provedores. Também há o teste `cargo test --lib media::gemini::tests::live_list_models -- --ignored` com `GEMINI_API_KEY`.
- Imagen, TTS e Veo dependem do que a sua conta libera. Uma operação sem modelo na conta aparece como indisponível.
- Vídeo e imagem podem gerar custo na sua conta Google, e o FORGE não estima esse custo.
- A transcrição pelo macOS depende das permissões de voz (pendência do M15).

---

## Relatório — Milestone 16: Harness

### Entregue
- **Classificação da tarefa** (regras pt/en, determinísticas):
  - atividade: pergunta, revisão, planejamento, documentação, testes, correção, refatoração, implementação ou design;
  - domínio: frontend, backend, banco, DevOps ou segurança;
  - porte: tamanho do texto e sinais como "typo/simples" ou "arquitetura/migrar/completo";
  - mostra as palavras que pesaram na decisão.
- **Recomendação explicada**, cobrindo cada item da especificação:
  - **perfil** pelo papel;
  - **provider**: só instalados e não deslogados, preferindo verificados e evitando os que falharam mais do que acertaram no histórico;
  - **modelo**: só aliases reais (haiku, sonnet, opus no Claude). Revisão, planejamento e segurança nunca ficam abaixo de sonnet. Nos outros providers fica o modelo padrão do CLI, sem nomes inventados;
  - **esforço**: limite de turnos real;
  - **permissão**: mínima, nunca "acesso total";
  - **skills e MCP** relevantes;
  - **worktree** para mudanças grandes;
  - **limites e uso** pelo **histórico real** dos últimos 30 dias (execuções, % concluídas, duração e custo **só quando informado**);
  - preferências **Economia, Equilíbrio e Qualidade**.
- **Painel Agente**: o botão ✨ (varinha) mostra a recomendação com os motivos e avisos. **"Aplicar nesta conversa"** ajusta perfil, permissão, worktree, provider, modelo, turnos, **skills e MCPs extras só desta execução** (novo no backend), e um chip mostra o ajuste ativo, com remoção.
- **Missões**: o interruptor "Harness: modelo por tarefa" (Maestro) escolhe o modelo do Claude pelo porte de cada tarefa quando o perfil não fixa um, e registra o motivo nos eventos. Migration 0011 (`missions.harness`).

### Testes
- Rust: 145 testes (+9):
  - classificação de 10 tarefas típicas e crescimento de porte;
  - papéis e níveis de modelo (piso de sonnet), turnos por preferência;
  - recomendação com provider deslogado substituído, com aviso;
  - histórico ruim desvia de provider, nenhum provider gera aviso, e o Gemini fica sem nome de modelo inventado;
  - agregação do histórico (custo só das execuções que o informaram, execuções em andamento ignoradas);
  - skills e MCPs extras (deduplicados, desativados ignorados, MCP só no Claude).
  - **Ponta a ponta no Maestro**: com Harness, a tarefa pequena roda com **`--model haiku`** e o motivo fica nos eventos; sem Harness, nenhum `--model`.
- **Ao vivo com os CLIs desta máquina**: Claude instalado e logado, Codex e Gemini deslogados. "Corrija um typo no botão de login" → Frontend Engineer · Claude · haiku · pode editar.
- Frontend: 63 testes. `tsc` ok e `cargo clippy --all-targets` sem avisos. `FORGE.app` reconstruído, e o banco real migrou para **v11**.

### Pendências conhecidas
- A classificação é por regras. Pedidos ambíguos caem em "implementação" de porte pelo tamanho, e o painel mostra os sinais para você julgar.
- "Esforço" é modelo + limite de turnos. Os CLIs não expõem outro controle que o FORGE tenha validado.
- Limites de plano e cota dos providers não são expostos pelos CLIs, então o FORGE usa só o histórico local, sem estimar cota.

---

## Relatório — Milestone 15: Voz (Jarvis + Ditado)

### Entregue
- **Helper nativo `forge-voice`** (Swift, framework Speech e AVAudioEngine):
  - compilado pelo `build.rs` e embutido como sidecar;
  - transcrições parciais e finais, nível do microfone, fim por silêncio (1,6 s), "parar" ou tempo máximo (90 s);
  - também transcreve arquivos de áudio.
  - Nenhum áudio é gravado em disco.
  - O `Info.plist` tem as justificativas de microfone e de fala, e o bundle ganhou **assinatura ad-hoc completa com o entitlement `audio-input`** (exigido pelo hardened runtime).
- **Jarvis** (⌘⇧J, botão de microfone na barra de título e paleta): interpretador determinístico pt-BR/en, com palavra de ativação opcional ("Forge, …"):
  - abrir projeto por nome, ou listar os projetos para escolher;
  - abrir telas e painéis;
  - **criar missão**: abre o diálogo com briefing e título preenchidos, e você confirma;
  - **verificar erros do frontend**: roda o Verificar do navegador na Prévia ou no Navegador e oferece "Enviar ao agente";
  - **pausar todos os agentes**: mostra o que será afetado e **exige confirmação**. Missões são pausadas (tarefas em andamento terminam) e execuções avulsas são interrompidas;
  - perguntar ao agente.
  - O que não é entendido não é adivinhado: o FORGE mostra o texto e oferece enviá-lo ao agente.
- **Ditado / Clock Voice** (⌘⇧D): fala com o agente em foco.
  - No painel Agente, o texto vai para o campo do pedido. Nos CLIs interativos (Claude, Codex, Gemini), vai direto para o terminal.
  - "Enviar automaticamente" é opcional.
- **Configurações → Voz**:
  - status das permissões e "Solicitar permissões";
  - idioma, com a marca "local" quando há reconhecimento no dispositivo;
  - **somente no dispositivo** (recusa em vez de enviar áudio);
  - respostas faladas (`say`, texto via stdin) e envio automático do ditado;
  - **autoteste**: sintetiza uma frase conhecida com `say`, transcreve e mede a taxa de acerto.

### Testes
- Rust: 136 testes (+6): parser das linhas do helper, validação de idioma, taxa de acerto sem acentos e pontuação, helper ausente gera erro real,
  **helper compilado respondendo status e listando pt-BR** e o plano de "pausar todos" (separa missões de execuções avulsas).
- Frontend: 63 testes (+9): os quatro exemplos da especificação, variantes de transcrição ("forj"), navegação, painéis, nome de projeto falado, missão em en/pt com acentos preservados,
  "não adivinha" e título derivado do briefing.
- `codesign --verify --deep --strict` ok, com o entitlement presente no app e no helper. `tsc` ok e `cargo clippy` sem avisos.

### Pendências conhecidas
- **A transcrição ao vivo ainda não foi validada.** O macOS pede permissão de microfone e de reconhecimento de fala ao FORGE na primeira vez, e isso exige o seu clique.
  Pedir pelo terminal atribuiria a permissão ao terminal, não ao FORGE. Passo: Configurações → Voz → "Solicitar permissões" → "Testar reconhecimento" → ⌘⇧J.
- O pt-BR não tem reconhecimento local nesta máquina: sem "Somente no dispositivo", o áudio é processado pela Apple.
- Builds de debug com assinatura ad-hoc podem fazer o macOS pedir a permissão de novo a cada novo build.
- Só macOS. Uma alternativa offline (whisper.cpp) fica como opção futura, com sua autorização para o download.

---

## Relatório — Milestone 14: Memória (OverMemory)

### Entregue
- **Memória compartilhada com escopo** Global, Workspace, Missão e Agente. Tipos: decisão arquitetural, convenção, contexto, histórico, erro recorrente, preferência e documentação.
  Cada item tem título, conteúdo, tags, fixação e origem (manual ou sugerida por um agente, com a execução de origem).
- **Isolamento entre projetos**: uma execução só vê:
  - memórias globais;
  - as do **próprio** workspace;
  - as da missão;
  - as do agente, presas ao workspace por padrão.
  - Global e "agente em todos os projetos" exigem **autorização explícita** na interface.
  - Remover um workspace apaga as memórias dele, e o aviso de remoção diz isso.
- **Os agentes recebem a memória automaticamente**, num bloco `<memory>`:
  - vale para o pane Agente, o planejador, as tarefas e o revisor do Maestro;
  - ordem: fixadas primeiro, depois agente → missão → workspace → global;
  - limite de 24 KB, e o que sobra é listado como "fora do limite".
  - Cada execução registra quais memórias usou (rodapé "memória: N").
- **Sugestões dos agentes**: o agente pode propor até 3 itens com `<forge-memory>`.
  - Ficam **inativas** em "Sugestões" até você aprovar, editar ou rejeitar.
  - Segredos são redigidos, eco do formato e duplicatas são ignorados, e a sidebar mostra as pendentes.
- **Tela Memória** (⌘8, sidebar e paleta):
  - abas Ativas, Sugestões e Arquivadas;
  - filtros por escopo, workspace e tipo, e busca em título, conteúdo e tags;
  - criar, editar, fixar, arquivar ou reativar, e excluir com confirmação.
  - O painel **"O que um agente recebe"** mostra exatamente o que o runner injeta para um workspace e agente.
- **Segredos**: conteúdo que parece credencial é **recusado** ao salvar.
- Migration 0010: tabela `memories` (com CHECK de consistência escopo↔alvo e cascatas) e `agent_runs.memory`.

### Testes
- Rust: 130 testes (+7):
  - Validação: alvos por escopo, tipo inválido, segredo recusado, missão herda o workspace e tags normalizadas.
  - **Isolamento**: a memória de w2 nunca aparece em w1, e a regra de agente com ou sem workspace é respeitada.
  - Composição: ordem, fixadas, limite e escape de atributos.
  - Parser de sugestões: placeholders, tipo inválido, duplicata, limite de 3 e segredo redigido.
  - Sugestões inativas até aprovar, escopo de missão, deduplicação, filtros, busca com `%`/`_` escapados e cascata ao remover o workspace.
  - **Ponta a ponta com o provider falso**: a memória chega ao `--append-system-prompt`, a de outro projeto não vaza, a execução registra a memória e a proposta vira sugestão ligada à execução.
- **Ao vivo com o Claude Code (haiku)**: a memória "codename ZEPHYR-42" mudou a resposta ("The internal codename is ZEPHYR-42."), e o `<forge-memory>` pedido virou uma sugestão pendente.
- Frontend: 54 testes (+2). `tsc` ok e `cargo clippy --all-targets` sem avisos.
- `FORGE.app` reconstruído e iniciado: o banco real migrou para **v10**.

### Pendências conhecidas
- A busca é por substring (LIKE). Busca semântica e FTS ficam para quando houver volume que justifique.
- Codex e Gemini recebem a memória como prefixo de instruções (não têm flag de system prompt). Só o Claude foi validado ao vivo.
- Ainda não há importação de memória a partir de `CLAUDE.md` ou `AGENTS.md`.

---

## Relatório — Milestone 13: Browser

### Entregue
- **Pane Navegador** (⌘⇧B ou paleta). Usa o Chrome instalado (Chrome, Chromium, Brave ou Edge, detectados nos caminhos padrão) em modo **headless com perfil isolado**
  (`<dados>/browser/profile`): sem cookies ou logins pessoais. Nada é baixado.
  - Barra de endereço com normalização (`localhost:3000` → http, domínio → https). `file:`, `javascript:`, `data:` e `chrome:` são **recusados**.
  - Voltar, avançar, recarregar (⇧ ignora o cache) e viewport "ajustar ao painel" ou presets (celular 390×844 com toque, tablet, notebook, desktop).
  - **Página ao vivo** via screencast, com clique, arraste, rolagem, teclado e colar repassados à página.
- **Console e rede**: `console.*`, exceções não tratadas, logs do navegador, **HTTP ≥ 400** e **falhas de rede** com a URL da requisição.
  Ficam só em memória (até 1000 por aba, nunca no disco). Há filtro "Problemas" e contador de erros na barra.
- **Capturas**: área visível ou página inteira (⇧), salvas em `<dados>/browser/shots/`, com "Revelar no Finder" (restrito a essa pasta).
- **Verificar página**: recarrega sem cache, espera o `load` e mais 1,5 s, e relata **tudo o que apareceu desde o recarregamento**: erros, avisos e captura.
  O relatório tem segredos redigidos. **"Enviar ao agente para corrigir"** pré-preenche o pane Agente com o relatório (marcado como conteúdo não confiável).
  O usuário revisa e escolhe a permissão, fechando o ciclo construir → rodar → verificar → capturar → corrigir → verificar de novo.
- **Pane Prévia**: lista os scripts do `package.json` (servidores primeiro; `vitest` não conta como `vite`) e detecta o gerenciador (campo `packageManager` ou lockfile: pnpm, yarn, bun, npm).
  O script roda numa **sessão de terminal real** (`<pm> run <script>`, sem shell, só scripts existentes). A URL local impressa é detectada (ANSI removido, `0.0.0.0` → `localhost`)
  e aberta automaticamente. A saída fica visível, com Parar e Reiniciar.
- Ciclo de vida: abas sobrevivem à troca de sessão. Fechar o pane fecha a aba. Abas órfãs são coletadas junto com os terminais. Remover o workspace fecha as abas dele.
  O Chrome é iniciado no primeiro uso, **encerrado quando a última aba fecha** e morto ao sair do FORGE.
- Sem migration: nada do navegador é persistido além do estado do pane (URL, viewport) e das capturas.

### Testes
- Rust: 123 testes (+15):
  - Normalização e recusa de URLs, endpoint CDP só em loopback e roteamento de respostas, erros e eventos do CDP.
  - Mapeamento de eventos para console (console, exceção com linha e coluna, HTTP 404, `loadingFailed` com URL, ruídos ignorados), corte de texto em fronteira UTF-8 e parser do `DevToolsActivePort`.
  - Argumentos (headless, perfil isolado, só 127.0.0.1), detecção de scripts e gerenciador, recusa de script inexistente e relatório em Markdown.
  - **Navegador ausente gera erro real**, sem simulação.
- **Teste ao vivo com o Chrome 156 desta máquina** (`cargo test --lib browser::manager::tests::live -- --ignored`), contra um servidor HTTP local:
  - abrir a página, título e frames de screencast;
  - **verificar**, que capturou o `console.error`, a exceção e o 404 do `fetch` com a URL, e gerou captura em PNG;
  - clique, foco e digitação (`value === "forge"` lido da página);
  - viewport mobile e captura da página inteira;
  - histórico voltar e avançar, e `file://` recusado;
  - **Chrome encerrado ao fechar a última aba**.
  - O teste revelou dois problemas, já corrigidos:
    - o screencast falhava durante a troca de processo da navegação (agora há nova tentativa);
    - um Chrome ficava órfão quando o teste entrava em pânico (agora `Drop` encerra o navegador).
- Frontend: 52 testes (+5): mapeamento de coordenadas com letterbox, teclado para CDP, detecção de URL de Vite e Next com ANSI, e moldura de "não confiável".
- `cargo clippy --all-targets` sem avisos. `tsc` e `vite build` ok. `FORGE.app` reconstruído e iniciado.

### Pendências conhecidas
- A interface do pane (screencast, teclado e prévia) foi verificada por tipos e build, e o motor ao vivo. **Um teste manual seu no app** é o que falta para a interação real com o mouse.
- Copiar texto da página (⌘C) e arrastar arquivos para a página ainda não são suportados. Colar (⌘V) funciona.
- Só navegadores Chromium. Safari e Firefox não expõem CDP.
- Agentes ainda não controlam o navegador diretamente: o caminho atual é o relatório. Um MCP do próprio FORGE para isso fica para um milestone futuro.

---

## Relatório — Milestone 12: MCP

### Entregue
- **Tela MCP** (⌘7): lista com status (verificado, falhou, não verificado), editor stdio ou HTTP (comando, argumentos, env, URL, cabeçalhos, segredos e somente leitura),
  **segredos no Keychain** (definir, substituir e remover, sem nunca exibir o valor), **Testar conexão** (handshake real com a lista de ferramentas, schemas e log) e associações.
- **8 modelos** da especificação: Filesystem, GitHub, Browser (Playwright), Supabase, PostgreSQL, Docker, Vercel e Cloudflare Docs, com requisitos explícitos.
- **Anexar a agentes, squads e missões** (chips nas três telas). Os servidores entram nas execuções do Claude Code (pane Agente e planejador, tarefas e revisor do Maestro)
  só com os escolhidos (`--strict-mcp-config`). O rodapé de cada execução mostra os MCPs usados.
- Migration 0009: `mcp_servers`, `mcp_links` e `agent_runs.mcp`.

### Testes
- Rust: 108 testes (+8):
  - Cliente contra um **servidor MCP falso** (Python): paginação, requisição servidor→cliente no meio do handshake, notificações e **segredo removido do log**.
    Também comando inexistente, servidor que encerra, timeout respeitado, URL `file://` recusada e parser de corpo SSE ou JSON.
  - Repositório: validação (placeholder não declarado, env inválida, segredo duplicado, http só em localhost), **segredos nunca no banco**, remoção do cofre ao deixar de declarar ou ao excluir,
    os 8 modelos criando servidores válidos, associações e composição, e configuração do Claude (servidor sem segredo é ignorado com motivo; permissão de ferramentas por leitura ou edição).
  - **Ponta a ponta**: um MCP anexado à missão chega ao agente via `--mcp-config` com o segredo resolvido, o arquivo tem **`-rw-------`** e **é apagado ao fim**,
    e a execução registra o MCP (4 de 4 repetições).
  - `cargo clippy --all-targets` sem avisos.
- **Testes ao vivo**:
  - Cloudflare Docs (HTTP remoto): handshake real com protocolo `2025-06-18` e 2 ferramentas.
  - **Claude Code real com MCP anexado**: chamou `mcp__cloudflare-docs__search_cloudflare_documentation` e respondeu com uma página real ("Cloudflare Workers KV").
    O arquivo temporário foi removido. Esse teste **revelou um bug** no nome usado em `--allowedTools` (eu convertia `-` em `_`), já corrigido.
- Frontend: 47 testes. `tsc` e `vite build` ok. `FORGE.app` reconstruído e iniciado: o banco real migrou para v9.

### Pendências conhecidas
- **Servidores stdio via `npx` não foram testados ao vivo**: baixar e executar o pacote externo foi bloqueado pela política de permissões desta sessão (ação que precisa do seu aval).
  O teste existe (`cargo test mcp::client::tests::live_filesystem -- --ignored`), e o caminho stdio está coberto pelo servidor falso.
- GitHub, Supabase e PostgreSQL dependem dos seus tokens. Vercel exige OAuth (não implementado). Docker exige Docker Desktop com MCP Toolkit (Docker não está instalado aqui).
- MCP por agente no Codex e no Gemini: planejado (os CLIs só têm configuração global).
- Leitura do Keychain por um app sem assinatura pode pedir autorização do macOS a cada novo build.

---

## Relatório — Milestone 11: Skills

### Entregue
- **Tela Skills** (⌘6): lista com ativa ou desativada, editor (nome, descrição, instruções), **histórico de versões** (ver e restaurar, sempre como nova
  versão), arquivos de apoio (scripts marcados como "nunca executado"), associações com agentes e squads, exportar como pasta, remover e restaurar padrão.
- **7 skills padrão** da especificação: React Expert, Next.js Expert, Supabase Expert, SaaS Architect, Security Auditor, UI Designer e Testing Expert.
- **Instalar** a partir de uma pasta (com `SKILL.md`) ou de um arquivo `SKILL.md`, com **prévia completa**: as instruções que serão enviadas aos agentes, os arquivos,
  avisos de risco (possível prompt injection, comandos de download e execução, credenciais, exfiltração, scripts) e a atualização de uma skill existente como nova versão.
- **Associações**: chips de skills no editor de agentes, no editor de squads e na missão ("Skills da missão").
- **Aplicação real**: as skills ativas do agente, do squad e da missão entram nas instruções de cada execução (pane Agente, planejador, tarefas e revisor do Maestro),
  em qualquer provider, sem gravar nada no repositório. O rodapé de cada execução mostra quais skills foram usadas.
- Migration 0008: `skills`, `skill_versions`, `skill_files`, `skill_links` e `agent_runs.skills`.

### Testes
- Rust: 100 testes (+9):
  - Parser (variantes de frontmatter, BOM, CRLF, blocos `>`, **aspas e escapes com ida e volta exata**: um teste mal escrito por mim expôs um bug real de escape, já corrigido),
    slug, heurísticas de risco e **varredura segura** (ignora `.git` e link simbólico, marca script, não guarda binário como texto, exige SKILL.md).
  - Repositório: as 7 padrão válidas e protegidas, versões (salvar sem mudança não versiona, restaurar cria versão nova, restaurar padrão), instalação nova e atualização
    (padrão protegida), associações e composição (deduplicação, desativadas ignoradas, alvo inválido recusado, limite de tamanho descartando a skill inteira).
  - **Ponta a ponta**: a skill associada à missão chega aos argumentos do agente da tarefa, a skill desativada não chega, e a execução registra `testing-expert@v1`.
  - Estável em 5 de 5 repetições. `cargo clippy --all-targets` sem avisos.
- **Teste ao vivo**: uma skill associada a um agente **mudou de fato o comportamento** do Claude Code real (haiku): ele terminou a resposta com o marcador exigido pela skill,
  e a execução registrou `signature@v1`.
- Frontend: 47 testes. `tsc` e `vite build` ok. `FORGE.app` reconstruído e iniciado: o banco real migrou para v8, com as 7 skills.

### Pendências conhecidas
- Instalação a partir de `.zip` e de repositórios remotos ainda não existe. O Marketplace (catálogo local e depois remoto) vem num milestone posterior.
- Os diálogos nativos e a interação visual não foram verificados automaticamente.
- As skills são injetadas como instruções, sem carregamento progressivo (sob demanda) de arquivos de referência. Por isso há o limite de 48 KB por execução.

---

## Relatório — Milestone 10: Squads + Recipes

### Entregue
- **Tela Squads & Receitas** (⌘5), com abas, editores e importação.
- **Squads**: os 5 da especificação (APP FACTORY, CINEMA SITE, SAAS FACTORY, AUTOPILOT, ARCADE) com membros, funções, etapas do fluxo, regras e
  modo padrão. Dá para criar, editar, duplicar, excluir os personalizados, restaurar os padrão e exportar.
- **8 novos perfis padrão** para completar os squads: Product Manager, Creative Director, Copywriter, Game Designer, Artist, Planner, Researcher e Executor (18 no total).
- **Integração com o Maestro**: a missão com squad é planejada por um membro do squad, segue o fluxo e as regras dele e só atribui tarefas aos seus membros.
  As regras também vão no prompt de cada tarefa. O squad pode ser escolhido ao criar a missão ou trocado antes do início.
- **Receitas**: as 8 da especificação (SaaS completo, Landing page premium, API, Refatorar, Corrigir bugs, Auditar segurança, App mobile e Migrar banco),
  3 delas com plano pronto. "Usar" abre um formulário com os campos, a prévia do briefing e o plano, e cria a missão já configurada (squad, modo e limites).
  O editor cobre campos, modelo de briefing, limites e remoção do plano. **"Salvar como receita"** reaproveita uma missão (briefing, squad, modo, limites e plano).
- **Compartilhar**: exportar e importar squads e receitas em `.forge.json` com diálogos nativos, prévia completa (instruções de cada agente, permissões, avisos)
  antes de importar e marcação "importado".
- Migration 0007: `squads`, `recipes`, origem dos perfis e squad e receita nas missões.

### Testes
- Rust: 91 testes (+11):
  - Squads: os 5 padrão válidos (todos os membros existem), CRUD e validação (membro duplicado, perfil inexistente, etapa inválida), restaurar e contexto de planejamento.
  - Receitas: as 8 padrão válidas, placeholders, validação (placeholder não declarado, limites, squad inexistente), criação com e sem plano pronto (campos
    obrigatórios, renderização sem `{{` residual, status, squad) e o caminho missão → receita.
  - Bundles: ida e volta de squad com perfil personalizado (novos IDs, nomes sem colisão, origem "import", **acesso total rebaixado**) e de receita com squad.
    Rejeições: JSON vazio, mais de 1 MB, versão e tipo inválidos, **campo desconhecido injetado**, referência pendente e provider desconhecido.
  - Motor: **o squad restringe o planejador e as atribuições** (um plano com papéis de fora do squad é redistribuído entre os membros do AUTOPILOT e planejado pelo Planner).
    Estável em 8 de 8 repetições.
  - Corrigido: transação aninhada em `tasks::replace`, agora com SAVEPOINT.
  - `cargo clippy --all-targets` sem avisos.
- Frontend: 47 testes. `tsc` e `vite build` ok.
- `FORGE.app` reconstruído e iniciado: o banco real migrou para v7, com 18 perfis, 5 squads e 8 receitas.

### Pendências conhecidas
- Os diálogos nativos de exportar e importar e a interação visual não foram verificados automaticamente.
- Ainda não existe um editor visual de plano dentro da receita (o caminho é "Salvar como receita" a partir de uma missão).
- Skills associadas a squads chegam com o Skills Engine (M11). O Marketplace remoto, no M14+ (a especificação o coloca depois).

---

## Relatório — Milestone 09: Maestro

### Entregue
- **Modos da missão**:
  - **Free**: você decide cada tarefa (a revisão é opcional).
  - **Squad**: o agente **Code Reviewer** revisa cada entrega contra os critérios de aceite e recomenda aprovar ou rejeitar, e você decide (a rejeição já vem
    com o feedback do revisor preenchido).
  - **Agentic**: o Maestro **aprova sozinho** o que o revisor aprova, **refaz com o feedback** o que for rejeitado ou falhar (até N tentativas),
    **reatribui** uma vez a outro provider disponível e **pede sua intervenção** quando não resolve.
- **Limites**: tentativas por tarefa, máximo de execuções por missão e orçamento (só custos informados pelos CLIs). Ao atingir um deles, a missão pausa com o motivo.
- **Gates sempre humanos**: aprovar o plano e integrar na sua branch.
- **Relatório final** (determinístico, sem LLM): tarefas, agentes e providers (com reatribuições), tentativas, branches, commits, revisões do Maestro,
  pendências e riscos, tokens e custo informado. Exibido na missão, com "Copiar" (Markdown).
- UI: seletor de modo e limites na missão, bloco "Maestro revisando… / recomenda aprovar / rejeitar" em cada tarefa (com a conversa do revisor),
  selo de reatribuição e card do relatório.
- Migration 0006: limites e relatório em `missions`, revisão, base e reatribuição em `mission_tasks`, e o tipo `review` em `mission_runs`.

### Testes
- Rust: 80 testes (+6). Incluem **3 novos testes de ponta a ponta** com git real e um agente falso:
  - **Agentic**: o revisor rejeita, o Maestro refaz com o feedback, o agente corrige, o revisor aprova, aprovação automática, a tarefa dependente segue e conclui,
    o relatório é gerado com os commits e a integração continua manual.
  - **Escalonamento**: 2 tentativas falham, o Maestro reatribui ao Codex (sondando só os outros providers), a 3ª falha e ele pede intervenção.
  - **Limite de execuções**: a missão pausa com o motivo, e retoma ao aumentar o limite.
  - Unidades: parser do veredito (inclusive com o formato ecoado), política de falha e relatório só com fatos.
  - Corrigidas **duas condições de corrida** expostas pelos testes (o status ficava visível antes do relatório e do motivo da pausa). A suíte passou **15 de 15** em execuções repetidas.
  - `cargo clippy --all-targets` sem avisos.
- **Teste ao vivo**: o Claude Code real (haiku), como revisor do Maestro, **rejeitou corretamente** um commit com bug (`add` retornando `a - b`), com explicação e correção
  precisas e no formato `<forge-review>` esperado.
- Frontend: 46 testes. `tsc` e `vite build` ok. `FORGE.app` reconstruído e iniciado, e o banco real migrou para v6.

### Pendências conhecidas
- Uma missão Agentic completa com agentes reais editando código não foi executada ao vivo (custo). Os componentes reais (planejador, execução e revisor)
  foram validados separadamente.
- A reatribuição só tem efeito com outro provider instalado **e logado** (hoje só o Claude está logado nesta máquina).
- Interação visual não verificada automaticamente.

---

## Relatório — Milestone 08: Mission Engine

### Entregue
- **Missão completa**: briefing → **plano gerado por IA** (Software Architect, somente leitura) ou manual → revisão e edição do plano
  (tarefas, agente, dependências, instruções e critérios de aceite) → **aprovar e iniciar** → tarefas executadas por agentes, **em paralelo quando
  independentes, cada uma em sua worktree** (partindo das branches das dependências) → **revisão** (aprovar, rejeitar com feedback, refazer, pular, cancelar)
  → missão concluída → **integrar** na branch base → limpar worktrees.
- Tela da missão com ações por estado, conversa ao vivo de cada tarefa e do planejamento (reaproveitando o RunView do M07), progresso, totais
  (execuções, tokens, tempo e custo informado, com quantas execuções informaram), configuração (paralelismo e revisão), briefing editável e
  histórico de eventos. Atualização em tempo real por evento do backend.
- Criar uma missão leva direto à tela dela. Lista de missões e dashboard abrem o detalhe.
- Migration 0005: novas colunas em `missions`, `mission_tasks`, `mission_events` e `mission_runs`.

### Testes
- Rust: 74 testes (+6). Incluem **dois testes de ponta a ponta do motor** com git real e um agente falso:
  - planejar (plano de 2 tarefas dependentes), impedir planejamento duplicado e início sem plano;
  - executar só a tarefa sem dependências, numa worktree, com commit automático e o checkout do usuário intocado;
  - rejeitar e refazer (nova worktree, feedback preservado, a antiga removida), aprovar, a 2ª tarefa partir da branch da 1ª (vê o arquivo dela), concluir;
  - integração recusada com o workspace sujo, depois merge das 2 branches, idempotente, e limpeza das worktrees;
  - tarefa com falha bloqueia os dependentes ("blocked"), pular desbloqueia e cancelar a missão interrompe tudo.
  - Estável em 6 repetições seguidas. Parser do plano (marcadores, cercas, papéis desconhecidos, chaves geradas), validação do DAG (ciclo,
    auto-dependência, inexistente, duplicada) e mascaramento nos eventos.
  - `cargo clippy --all-targets` sem avisos.
- **Teste ao vivo**: o Claude Code real (haiku) planejou uma missão num repositório temporário e gerou **3 tarefas válidas** (Backend implementa,
  QA testa, Revisor revisa) com as dependências corretas, em 18 s.
- Frontend: 46 testes. `tsc` e `vite build` ok.
- `FORGE.app` reconstruído e iniciado. O banco real migrou para v5.

### Pendências conhecidas
- A execução real de tarefas com edição não foi rodada ao vivo neste milestone (consumiria mais do seu plano). Ela usa o mesmo runner validado ao vivo no M07,
  e a orquestração está coberta pelos testes de ponta a ponta.
- Interação visual não verificada automaticamente.
- Reatribuição automática, revisão por agente, tratamento automático de falhas e os modos Free, Squad e Agentic com autonomia diferente ficam para o **Maestro (M09)**.

---

## Relatório — Milestone 07: Agent Engine

### Entregue
- **Tela Agentes** (⌘3): cadastro de perfis com nome, função, descrição, provider, modelo (com aliases), máximo de turnos (Claude), permissão
  padrão e instruções. Permite criar, duplicar, excluir os personalizados e restaurar os padrão. Os **10 perfis da especificação** já vêm instalados,
  e há uma lista de **execuções recentes** de todos os workspaces.
- **Pane Agente** (⌘I): escolha do perfil, permissão (leitura, edição ou total, esta com confirmação), opção de **worktree isolada**, conversa com
  **streaming ao vivo** (texto, chamadas de ferramenta com entrada e saída recolhíveis, avisos e erros), rodapé com status, modelo, duração,
  tokens e custo informado, **cancelar** e **alterações** (arquivos e commits desde o início, abrir no editor ou abrir a worktree).
  **Continuar conversa** retoma a sessão nativa no mesmo diretório.
- **Runner**: processo em grupo próprio (cancelar também encerra subprocessos), timeout de 30 min, limite de 8 simultâneos, eventos normalizados
  e persistidos com **segredos mascarados**, e marcação de execuções interrompidas ao reabrir o app.
- Dashboard: o card "Agentes em execução" inclui as execuções sem interface. Sidebar com contador de execuções ativas, paleta, atalhos e layout embutido **Agente**.
- Migration 0004: `agent_profiles`, `agent_runs` e `agent_events`.

### Testes
- Rust: 68 testes (+17):
  - **Normalização** contra uma saída real do Claude com uso de ferramenta (fixture higienizada), o stream real de erro 401 do Codex, o formato
    de item documentado do Codex e o formato do Gemini extraído do código-fonte do CLI instalado.
  - **Pipeline completo** com um CLI falso que reproduz o stream real: spawn, parse, persistência, assinatura ao vivo (todo evento entregue
    pelo snapshot ou ao vivo), resumo final e reabertura de execução finalizada. Também: **cancelamento** mata o grupo de processos (< 6 s), falha
    traz a mensagem do stderr e execuções órfãs são marcadas.
  - Mapeamento de permissões e flags por provider (prompt nunca vira flag e modelo malicioso é rejeitado), acesso total exige confirmação,
    perfis (10 padrão idempotentes, edição preservada, restaurar e validações) e **mascaramento de segredos** (12 formatos, sem falso positivo em código).
  - `cargo clippy --all-targets` sem avisos.
- **Teste ao vivo** (`cargo test agents::runner::tests::live -- --ignored`): Claude Code real **leu um arquivo com ferramenta** e respondeu
  "PINEAPPLE" (eventos `session → tool_use → tool_result → text → result`). A segunda execução **continuou a sessão** e respondeu "pineapple".
- Frontend: 45 testes. `tsc` e `vite build` ok (o pane Agente é carregado sob demanda, com 16,5 kB).
- `FORGE.app` reconstruído e iniciado: o banco real migrou para v4, com os 10 perfis padrão.

### Pendências conhecidas
- Codex e Gemini: execuções reais dependem do seu login. O parser de sucesso deles segue o formato documentado ou extraído do código-fonte.
- Gemini não continua conversas (limitação do CLI: retomada só por índice ou "latest").
- Interação visual não verificada automaticamente (sem captura de tela ou automação de UI neste Mac).
- Fora do escopo agora: skills e memória por agente (M11 e M14), seleção automática de configuração (Harness, M16) e orquestração entre agentes (M08 e M09).

---

## Relatório — Milestone 06: Providers

### Entregue
- **Tela Providers** (⌘3): para Claude Code, Codex e Gemini mostra versão, binário, **estado de login** (método e plano), modelo configurado,
  aliases, capacidades, último teste real (duração, tokens e custo somente quando informados pelo CLI), "Testar conexão" com
  confirmação, "Fazer login" e "Abrir pane". Integrações ainda não implementadas (OpenCode, Ollama, LM Studio, OpenRouter, DeepSeek, Custom CLI)
  aparecem como "ainda não integrado", com o status de instalação real.
- **Panes de agente** (Claude Code, Codex, Gemini): o CLI oficial interativo roda no PTY, na pasta do workspace, com a autenticação nativa e o
  ambiente do seu login shell. A barra mostra versão, login, verificado, campo de modelo (com aliases), reiniciar, **modo login** (roda
  `claude auth login`, `codex login` ou o fluxo do `gemini` dentro do pane) e "Iniciar agente".
- **Modo sem interface** com saída estruturada para os três (base do M07), com parsers tolerantes.
- Dashboard: o card Providers mostra login e verificação reais, e o card **Agentes em execução** lista os agentes vivos (provider, workspace, tempo).
  Sidebar, paleta (abrir cada agente) e atalho ⌘3.
- Migration 0003: `provider_checks`.
- **Instalados neste Mac** (a seu pedido): Codex CLI 0.166.0 e Gemini CLI 0.66.0 (`npm i -g`, prefixo `~/.local/node`). O npm bloqueou os scripts
  nativos opcionais do Gemini (`node-pty`, `keytar`), que não foram autorizados.

### Testes
- Rust: 51 testes (+9): parsers contra **saídas reais higienizadas** (stream do Claude bem-sucedido, Codex 401, Gemini sem autenticação), formato
  de sucesso documentado de Codex e Gemini, status de login (o e-mail do Claude não é mantido), detecção do Gemini por presença de arquivos ou variáveis,
  modelo configurado (`config.toml` e `settings.json`), validação de modelo (bloqueia injeção de flag) e persistência das verificações.
  `cargo clippy --all-targets` sem avisos.
- **Testes ao vivo** (`cargo test providers::tests::live -- --ignored`), executados nesta máquina:
  - Claude Code 2.1.317: logado (claude.ai, Pro), com **verificação real OK** ("FORGE_OK", claude-haiku-5-5, 4,6 s).
  - Codex 0.166.0: não logado, e a verificação falha corretamente com 401.
  - Gemini 0.66.0: não logado, e a verificação falha corretamente pedindo o método de autenticação.
- Frontend: 45 testes (+1: agentes contam na coleta de PTYs órfãos). `tsc` e `vite build` ok (o AgentPane é carregado sob demanda, com 4,8 kB).
- `FORGE.app` reconstruído e iniciado. O banco real foi migrado para `user_version = 3`.

### Pendências conhecidas
- Codex e Gemini: o caminho de **sucesso** não foi verificado (sem login). Depois que você fizer login, "Testar conexão" valida ao vivo.
- O teste ao vivo do Claude rodou pelo `cargo test` e não ficou gravado no banco do app. Clique em "Testar conexão" para aparecer o selo de verificado.
- Interação visual dos panes de agente não verificada automaticamente (sem captura de tela ou automação de UI neste Mac).
- Lista de modelos disponíveis: os CLIs não expõem um catálogo de modelos, então o FORGE mostra o modelo configurado e os aliases documentados, sem inventar uma lista.

---

## Relatório — Milestone 05: Git Center + Worktrees

### Entregue
- **Pane Git** (⌘⇧G), com cabeçalho mostrando branch atual (troca rápida e nova branch), upstream, fetch, pull ↓n (fast-forward) e push ↑n,
  ou "Publicar" quando não há upstream.
- **Alterações**: seções de conflitos, staged e alterações. Stage, unstage e descarte por arquivo e em lote. Commit com mensagem (⌘↵) e amend.
  Banner de merge em andamento com "Abortar merge". Abrir o arquivo no editor direto da lista.
- **Diff** lado a lado ou em linha (Monaco), com regiões sem mudança recolhidas: HEAD ↔ stage, stage ↔ arquivo, arquivo novo, conflito e commits do histórico.
- **Stage, unstage e descarte por seleção de linhas** no diff, sem tocar no restante do arquivo.
- **Branches**: locais e remotas (ativar cria tracking), nova branch a partir de qualquer base, merge na atual (conflitos vão para Alterações)
  e excluir com proteção para commits não integrados.
- **Histórico**: os últimos 300 commits com refs. Expandir mostra os arquivos, e clicar mostra o diff do commit.
- **Stash**: guardar (inclui arquivos novos) com mensagem, aplicar, pop e excluir (com confirmação).
- **Worktrees**: listar, criar (nova branch ou branch existente) na pasta gerenciada, abrir como workspace "Repo ⎇ branch"
  e remover (protege a principal e pede confirmação extra com alterações pendentes).
- Repositório inexistente vira o botão "Inicializar repositório". O badge de branch do workspace acompanha as operações, e os buffers do editor
  são conferidos após trocar de branch, fazer pull, stash, merge ou descartar.
- Layout embutido **Revisão** (Git + editor + terminal) e entradas na paleta.

### Testes
- Rust: 42 testes (+7), com **git real** em repositórios temporários: status e stage/unstage (inclusive sem HEAD), commit e mensagem vazia,
  stage parcial pelo índice, descartar, arquivos de um commit, criar e trocar branch, nome inválido, merge com conflito e abort,
  excluir branch não integrada (exige force), stash, push sem remoto, ciclo completo de worktree e repositório inexistente com `init`.
  Também os parsers de status v2 e worktree, a validação de caminhos e refs (injeção de opção, `..`) e a ocultação de credenciais.
  `cargo clippy --all-targets` sem avisos.
- Frontend: 44 testes (+5 para o stage por linhas: modificação, inserção, deleção, CRLF e seleção). `tsc` e `vite build` ok (o Git Pane é
  carregado sob demanda, com 37 kB).
- `FORGE.app` reconstruído e iniciado.

### Pendências conhecidas
- Interação visual não verificada automaticamente (sem captura de tela ou automação de UI neste Mac).
- Fetch, pull e push não têm teste automatizado contra um remoto real. O caminho de erro (sem remoto) está coberto.
- Descartar arquivo novo (lixeira) não roda nos testes, para não sujar a lixeira do usuário.
- Fora do escopo agora: rebase interativo, cherry-pick, blame, grafo de commits e editor de conflitos com 3 painéis
  (hoje o arquivo em conflito abre no editor e, após resolver, é feito o stage).

---

## Relatório — Milestone 04: Editor

### Entregue
- **Pane Editor** com Monaco: abas (indicador de não salvo, nome da pasta quando dois arquivos têm o mesmo nome, clique do meio fecha),
  tema FORGE, Geist Mono, minimapa, sticky scroll, guias de colchetes, linguagem detectada pela extensão e posição do cursor
  restaurada por aba. A barra de status mostra caminho, Ln/Col, seleção, UTF-8, LF/CRLF e linguagem.
- **Salvar** (⌘S) com gravação atômica, **Salvar todos** (⌘⌥S) e confirmação ao fechar aba ou janela com alterações pendentes.
- **Conflitos**: arquivos alterados fora do FORGE são detectados ao salvar, ao focar a janela e ao trocar de aba, com as opções Recarregar
  e Sobrescrever. Buffers sem alteração recarregam sozinhos.
- Arquivos maiores que 5 MB abrem somente leitura e binários mostram um aviso: nunca há gravação parcial.
- **Quick Open** (⌘P) com busca fuzzy e destaque dos caracteres encontrados.
- **Pane Busca** (⌘⇧F): texto ou regex, maiúsculas e palavra inteira, resultados agrupados por arquivo e clique que abre o arquivo na linha e coluna exatas.
- **Explorer** reescrito: só a árvore, e o clique abre no editor. Destaca o arquivo ativo e marca arquivos não salvos. Tem novo arquivo e nova pasta
  (na raiz, na pasta selecionada ou pelo menu da pasta), renomear, copiar caminho e mover para a lixeira com confirmação.
  Renomear e excluir atualizam as abas de todas as sessões e preservam edições não salvas.
- O layout padrão de novos workspaces passa a ser **Files | Editor | Overview**. O Editor e a Busca são carregados sob demanda.

### Testes
- Rust: 35 testes (+8): gravação atômica (sem temporários sobrando), conflito por mtime, criação via gravação,
  preservação de permissão 0755, guardas contra traversal e symlink para fora do workspace na escrita, criar sem sobrescrever,
  renomear (sem sobrescrever, sem mover para dentro de si mesmo, sem sair do workspace), proteção da raiz na lixeira, lista de arquivos respeitando
  `.gitignore`, busca (maiúsculas, palavra, regex, regex inválida, limite) e colunas UTF-16 com acentos. `cargo clippy --all-targets` sem avisos.
- Frontend: 39 testes (+10): matcher fuzzy, abrir no editor (cria, reaproveita, restaura minimizado, deduplica abas, revela linha)
  e renomear ou fechar abas em todas as sessões. `tsc` e `vite build` ok: o bundle principal tem 534 kB, o Monaco e os workers são separados e locais,
  e o CSS e a fonte codicon foram empacotados.
- `FORGE.app` reconstruído e iniciado.

### Pendências conhecidas
- O uso interativo do Monaco dentro da janela não foi verificado automaticamente (sem captura de tela ou automação de UI neste Mac).
- A ida real para a lixeira (`trash::delete`) não roda nos testes automatizados para não sujar a lixeira do usuário. As guardas de caminho, sim.
- Ainda não há substituição em massa na busca, nem watcher de arquivos em tempo real (a detecção acontece ao focar, trocar de aba ou salvar), nem LSP.
- Workers do Monaco somam cerca de 9 MB no pacote, aceitável para um app desktop.

---

## Relatório — Milestone 03: Terminal

### Entregue
- **Pane Terminal** real: PTY nativo (portable-pty) mais xterm.js com tema FORGE, Geist Mono, 10 mil linhas de rolagem, links
  clicáveis, colar texto grande em blocos e redimensionamento contínuo (o PTY recebe linhas e colunas).
- Shell do usuário como login shell, no diretório do workspace, com o PATH completo (o mesmo da detecção de ambiente).
- **Persistência durante a execução**: trocar de aba, sessão ou workspace, minimizar ou reorganizar o layout não interrompe o processo.
  Ao voltar, o pane reconecta e recebe o histórico (scrollback de 512 KB).
- **Estados do pane**: iniciando, em execução, encerrado (com código de saída e "Reiniciar") e sessão perdida após reiniciar o app
  (com "Novo terminal"). Há um rodapé com shell, diretório e pid.
- **Processos**: Configurações → Processos lista todos os terminais (workspace, diretório, pid, tempo de vida, status) com "Encerrar".
- **Limpeza**: fechar o pane mata o processo, órfãos são coletados, e remover o workspace ou sair do app encerra tudo.
- Layout embutido **Desenvolvimento** (arquivos + overview + terminal). Atalho ⌘J para novo terminal abaixo, e entradas na paleta.
- O pane Terminal é carregado sob demanda: o bundle principal tem 555 kB e o terminal, 340 kB separados.

### Testes
- Rust: 27 testes (+5). Incluem um **PTY real**: executa comando e lê a saída, confere o `pwd` no workspace, `stty size` após
  resize, reconexão com scrollback, token de detach obsoleto, código de saída (`exit 3` → 3), escrita recusada após sair,
  kill com verificação no SO de que o processo morreu, e decoder UTF-8 com bytes partidos e inválidos. Também o anel de scrollback e a rejeição de diretório inexistente.
  `cargo clippy --all-targets` sem avisos.
- Frontend: 29 testes (+2: estado efêmero em cópias e coleta de ids referenciados em todas as sessões e minimizados). `tsc` e `vite build` ok.
- `FORGE.app` reconstruído e iniciado sem erro.

### Pendências conhecidas
- O caminho completo da UI (digitar no xterm dentro da janela) não foi exercitado automaticamente: sem captura de tela nem
  automação de UI neste Mac. A ponte Rust ↔ PTY está coberta por testes, mas o Channel Tauri ↔ xterm não.
- Não validado no Windows (ConPTY) nem no Linux.
- Busca no terminal, temas por usuário e escolha de shell ficam para depois. Agentes CLI rodando no PTY entram no M06.

---

## Relatório — Milestone 02: Panes

### Entregue
- **Arrastar e soltar**: arraste o pane pelo cabeçalho (alça ⋮⋮) e solte numa borda de outro pane para encaixar
  (esquerda, direita, cima, baixo) ou no centro para trocar de lugar. Há preview da zona, um rótulo que segue o cursor e Esc cancela.
- **Minimizar e restaurar**: botão "–" no pane. A bandeja "Minimizados" no rodapé restaura o pane ao lado do vizinho original ou o fecha.
- **Várias sessões por workspace**: abas com criar (+ ou ⌘T), renomear (duplo clique), duplicar (mantém o estado dos panes),
  mover para a esquerda ou direita, fechar (a última é protegida) e alternar (⌘⇧] / ⌘⇧[). A sessão ativa é restaurada ao abrir o workspace.
- **Layouts**: 5 embutidos (Foco, Lado a lado, Três colunas, Grade 2×2, Principal + lateral). Também dá para salvar o layout atual com nome,
  aplicar em qualquer workspace com "Desfazer" e renomear ou excluir em Configurações → Layouts.
- **Paleta ⌘K**: nova sessão, ir para sessão, aplicar layout, minimizar pane e restaurar minimizados.
- Migration 0002: `workspace_sessions` e `layout_presets`, e `workspaces.active_session_id`. A tabela `pane_layouts` foi migrada e removida.

### Testes
- Rust: 22 testes (+4: sessões, layouts salvos, migração v1→v2 com dados). `cargo clippy --all-targets` sem avisos.
- Frontend: 27 testes (+14: mover nas 4 bordas e no centro, colapso, zonas de soltura, minimizar/restaurar com âncora perdida,
  limite de panes contando os minimizados, serialização da bandeja, templates com ids únicos, presets válidos). `tsc` e `vite build` ok.
- `FORGE.app` reconstruído e iniciado sobre o banco real do M01: migrou de `user_version` 1 para 2 sem erro.

### Pendências conhecidas
- A interação visual (arrastar, abas) não foi verificada por captura de tela (permissão do macOS). Os testes cobrem a lógica,
  não a renderização.
- As abas de sessão são reordenadas por menu (sem arrastar).
- O arrasto de panes funciona dentro da sessão atual. Mover entre sessões ou janelas fica para depois.
- Panes de Files recarregam a árvore quando a estrutura muda (o estado de pastas abertas é preservado).

---

## Relatório — Milestone 01: Foundation

### Entregue
- App desktop Tauri 2 (React 19, TS, Vite 8, Tailwind 4, Zustand 5). Janela com title bar overlay no macOS.
- Design system FORGE (tokens, Geist, primitivos estilo shadcn) e slots da marca com fallback tipográfico.
- **Onboarding** em 3 etapas: boas-vindas, verificação do ambiente e primeiro workspace.
- **Dashboard** com NEW MISSION, OPEN WORKSPACE, workspaces recentes, providers detectados, missões, atividade,
  agentes (vazio até o M07) e consumo (sem dados, nunca estimado).
- **Workspaces**: seletor nativo de pasta, registro em SQLite, fixar, remover da lista, mostrar no Finder,
  branch Git lida de `.git/HEAD` (inclui worktrees) e restauração da última sessão ao iniciar.
- **Panes (base)**: dividir à direita/abaixo, redimensionar, maximizar (duplo clique no header), duplicar,
  trocar tipo, fechar com colapso de splits e persistência por workspace. Tipos disponíveis: Files
  (árvore lazy + visualização somente leitura) e Overview.
- **Missões**: criação como rascunho (Free/Squad/Agentic), listagem, cancelamento e exclusão com confirmação.
- **Configurações**: restaurar sessão, pasta padrão, mascote, reduzir movimento, ambiente, status da marca, dados e sobre.
- **Command palette** (⌘/Ctrl+K) e atalhos: N nova missão · O abrir workspace · , configurações · 1/2 navegar ·
  [ ] alternar pane · ⇧↵ maximizar.
- SQLite com migration 0001: `app_settings`, `workspaces`, `pane_layouts`, `missions`, `activity_events`.

### Testes
- Rust: 18 testes (migrations, settings, workspaces, layouts, missões, atividade, sandbox de FS incluindo traversal e symlink,
  timeout de processos, parsing de versões, detecção completa). `cargo clippy --all-targets` sem avisos.
- Frontend: 13 testes do modelo de layout (vitest). `tsc` sem erros. `vite build` ok.
- `tauri build --debug --bundles app` gerou `FORGE.app`. O app abriu e criou e migrou o banco (`user_version = 1`).

### Pendências conhecidas
- **Arquivos oficiais da marca ausentes**: logo e mascote não foram encontrados. Ícones do app são provisórios.
- Validação visual do app não foi automatizada (captura de tela bloqueada pela permissão do macOS).
- Ainda não validado em Windows e Linux (o código é multiplataforma, mas o build não foi testado lá).
- Bundle JS com 523 kB (sem code-splitting). Avaliar no M04 com o Monaco.
