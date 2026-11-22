## Resumen

Describe el cambio y qué sección de [docs/PARADIXE_SOFT.md](../docs/PARADIXE_SOFT.md) implementa o actualiza.

## Checklist SDD

- [ ] Aplica **Contract-First** ([openapi/openapi-v1.json](../openapi/openapi-v1.json) y `npm run gen:api` actualizados si toca API).
- [ ] Cálculos en metros en SQL usan `::geography` (nunca grados decimales).
- [ ] Cálculos decimales en Rust usan `Decimal` con `MidpointAwayFromZero`.
- [ ] Sin secretos, tokens, OTPs ni `user_id` en pings o logs.
- [ ] Estados *Fail-Safe* (`NO_SAFE_ROUTE` / `STALE_IVV` / `UNVERIFIED_PHYSICAL_ATTRIBUTES`) preservados.
- [ ] Pasaron localmente `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --workspace` y `npm run typecheck && npm test && npm run build`.
