# Desenvolvimento

## Requisitos

- macOS 12+ com Xcode Command Line Tools (`xcode-select --install`). O `swiftc` compila o helper de voz no `build.rs`.
- Node 24 e npm.
- Rust stable (rustup).
- Opcional, para recursos ao vivo: Google Chrome, git, Claude Code, Codex e Gemini CLI.

## Comandos

```sh
npm ci
npm run app:dev        # app em modo desenvolvimento (Vite + Tauri)
npm run typecheck      # tsc
npm test               # vitest
cd src-tauri && cargo test            # testes do núcleo
cd src-tauri && cargo clippy --all-targets -- -D warnings
npm run app:build      # build de release (.app + .dmg)
```

### Testes ao vivo

Alguns testes ficam marcados como `#[ignore]` porque usam CLIs reais, o Chrome ou chaves. Para rodá-los:

```sh
cd src-tauri && cargo test --lib <nome> -- --ignored --nocapture
```

| Teste | Requisito |
|---|---|
| `browser::manager::tests::live_*` | Chrome instalado |
| `agents::runner::tests::live_*` e `observability::live::*` | Claude Code logado |
| `media::gemini::tests::live_list_models` | `GEMINI_API_KEY` |
| `voice::tests::live_*` | permissão de reconhecimento de fala |

## Estrutura

- `src/`: frontend React/TypeScript. Toda chamada ao núcleo passa por `src/lib/ipc.ts`.
- `src-tauri/src/`: núcleo Rust, um módulo por domínio (agents, missions, browser, memory, voice, harness, media, autopilot, observability…).
- `src-tauri/migrations/`: migrations SQLite apenas para frente (`PRAGMA user_version`).
- `src-tauri/voice/main.swift`: helper nativo de voz (sidecar `forge-voice`).
- Documentação: `docs/ARCHITECTURE.md` (arquitetura), `docs/DECISIONS.md` (ADRs, só acréscimos) e `docs/ROADMAP.md` (relatórios por milestone).

## Regras do projeto

- Sem funcionalidades falsas, dados fictícios ou mocks permanentes. O que não está configurado aparece como indisponível.
- Nunca registrar segredos: use `redact::redact_secrets` em tudo o que é persistido ou exibido.
- Menor privilégio por padrão. Operações destrutivas exigem confirmação.
- Toda decisão técnica relevante vira um ADR em `docs/DECISIONS.md`.
