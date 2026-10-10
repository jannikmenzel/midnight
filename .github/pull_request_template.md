## What and why

<!-- What does this change do, and why is it needed? Link related issues. -->

## Checklist

- [ ] `npm run build` passes (includes the TypeScript typecheck)
- [ ] `cargo clippy --all-targets -- -D warnings` and `cargo test` pass in `src-tauri/`
- [ ] New UI strings have a German translation in `src/lib/i18n.de.ts`
- [ ] New backend messages use `tr!("English", "Deutsch")`
- [ ] Schema changes come with a new migration in `src-tauri/migrations/`
