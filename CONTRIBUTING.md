# Guía de Contribución — PARADIXE SOFT

Gracias por contribuir al desarrollo de **PARADIXE SOFT** (**TakumiSoft**). Este proyecto sigue **Specification-Driven Development (SDD)**: toda modificación de código debe ser trazable a [docs/PARADIXE_SOFT.md](./docs/PARADIXE_SOFT.md).

## Principios Obligatorios

1. **Contract-First y Test-First (TDD):**
   * Actualiza primero [openapi/openapi-v1.json](./openapi/openapi-v1.json) o la migración SQL correspondiente.
   * Escribe la prueba de contrato/integración que falle (**RED**) antes de implementar el cambio hasta pasar (**GREEN**).
2. **Núcleo Rust (`crates/`):**
   * Usa `#![forbid(unsafe_code)]` y `rust_decimal::Decimal` con `RoundingStrategy::MidpointAwayFromZero` para aritmética financiera/geotécnica idéntica a `NUMERIC` de PostgreSQL.
   * Todo motor (`ivv`, `consensus`, `routing`, `impact`) debe mantenerse utilizable tanto como librería como mediante su binario CLI JSON (`stdin`/`stdout`).
   * Ejecuta antes de subir cambios:
     ```bash
     cargo fmt --all -- --check
     cargo clippy --workspace --all-targets -- -D warnings
     cargo test --workspace
     ```
3. **PostGIS Métrico y Sincronización:**
   * Todas las mediciones en metros (`ST_DWithin`, `ST_Buffer`, `ST_Length`) deben convertirse explícitamente a `::geography`.
   * Prohibido `BIGSERIAL`/`SERIAL` en `sync_change_log`; toda mutación sincronizable incrementa `sync_control` bajo bloqueo transaccional en la misma transacción.
4. **Frontend PWA (`apps/web/`):**
   * Consulta y aplica la skill [Modern Web Guidance](./.agents/skills/modern-web-guidance/SKILL.md) para cualquier cambio de HTML, CSS o APIs cliente.
   * Regenera tipos desde OpenAPI antes de compilar:
     ```bash
     cd apps/web
     npm run gen:api && npm run typecheck && npm test && npm run build
     ```
5. **Convención de Commits:**
   * Usa **Conventional Commits** (`feat(rust): ...`, `fix(web): ...`, `docs: ...`, `test(routing): ...`).
