# Changelog

## v0.1.0 — 2026-11-03

Primeira versão pública. Os relatórios detalhados de cada milestone estão em `docs/ROADMAP.md`.

- **M01–M05:**
  - fundação (Tauri 2, design system, SQLite);
  - painéis com arrastar e soltar e sessões;
  - terminal PTY;
  - editor Monaco;
  - Git Center com worktrees.
- **M06–M10:**
  - providers (Claude Code, Codex, Gemini CLI) com verificação real;
  - Agent Engine;
  - Mission Engine;
  - Maestro (revisão, tentativas, escalonamento);
  - squads e receitas.
- **M11–M14:**
  - skills (SKILL.md, versões);
  - MCP (stdio e HTTP, Keychain, anexo por execução);
  - navegador (CDP, console, capturas, verificar e corrigir);
  - memória compartilhada com escopos e sugestões revisadas.
- **M15–M17:**
  - voz (Jarvis e ditado, com o reconhecimento de fala do macOS);
  - Harness (configuração por tarefa, explicada);
  - mídia (Gemini, análise pelo Claude Code, transcrição local).
- **M18–M20:**
  - Autopilot (rotinas com limites e aprovação de comandos);
  - observabilidade (métricas reais, processos, histórico com reprodução, auditoria);
  - produtização (atualizador assinado via GitHub Releases, instância única, relatórios de falha locais, CI e release, documentação).

### Limitações conhecidas

- Sem notarização Apple: a primeira abertura exige "Abrir" manual.
- As rotinas do Autopilot só rodam com o app aberto.
- A logo e o mascote oficiais ainda não foram incluídos. O app usa um fallback tipográfico até receber os arquivos originais.
