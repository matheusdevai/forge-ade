# FORGE — Build beyond limits.

**Agentic Development Environment.** App desktop para comandar, coordenar e acompanhar times de agentes de IA (Claude Code, Codex, Gemini CLI) trabalhando em projetos reais: terminal, editor, git, missões com worktrees, navegador, memória, voz, mídia, automações e observabilidade.

> Status: **v0.1.0**, com os 20 milestones do roadmap concluídos. **Plataforma suportada e testada: macOS.** Windows e Linux compilam pelo Tauri, mas recursos como voz e assinatura são específicos do macOS e não foram validados. Veja [docs/ROADMAP.md](docs/ROADMAP.md).

## Instalar

Baixe o `.dmg` na [última release](https://github.com/matheusdevai/forge-ade/releases/latest) e siga o [guia do usuário](docs/USER_GUIDE.md#instalação-macos). O app ainda não é notarizado pela Apple, então a primeira abertura exige **botão direito → Abrir**. As atualizações seguintes são feitas pelo próprio app, com assinatura verificada.

## Desenvolver

```bash
npm ci
npm run app:dev        # app desktop com hot reload
npm test && npm run typecheck
(cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings)
npm run app:build      # .app + .dmg de release
```

Requisitos: macOS com Xcode Command Line Tools, Node 24 e Rust stable. Detalhes em [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Identidade visual

Os arquivos oficiais ficam em `assets/brand/` (veja [assets/brand/README.md](assets/brand/README.md)). Sem eles, a interface usa apenas um wordmark tipográfico, nunca imagens genéricas.

## Documentação

- [Guia do usuário](docs/USER_GUIDE.md)
- [Desenvolvimento](docs/DEVELOPMENT.md)
- [Como publicar uma versão](docs/RELEASING.md)
- [Segurança](SECURITY.md)
- [Arquitetura](docs/ARCHITECTURE.md)
- [Decisões técnicas (ADR)](docs/DECISIONS.md)
- [Design system](docs/DESIGN_SYSTEM.md)
- [Roadmap e relatórios de milestone](docs/ROADMAP.md)
- [Changelog](CHANGELOG.md)
