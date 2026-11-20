# PARADIXE-SOFT — QULLA-RUTA

**Red Territorial de Resiliencia Vial y Logística Crítica (MVP)**

Sistema de predicción geotécnica, detección de incidentes y enrutamiento resiliente para el corredor **Huánuco – Carpish – Tingo María** bajo metodología **Specification-Driven Development (SDD)**.

---

## Módulos Implementados (Track C: Motor Geotécnico IVV & Simulador WFEO)

1. **Motor de Vulnerabilidad Vial ($IVV$):** Cálculo determinista y reproducible cada 3 horas clasificando tramos en `GREEN`, `AMBER` y `RED` según lluvia acumulada 24/72h, pendientes, quebradas, tipo de suelo y puntos críticos históricos.
2. **Notification Firewall:** Aislamiento estricto de seguridad que intercepta y aborta cualquier despacho a pasarelas reales (SMS/WhatsApp) en contexto `SIMULATION`.
3. **Métricas de Impacto con Signo Libre ($\Delta \in \mathbb{R}$):** Cálculo fiel de $\Delta T$, $\Delta \text{CO}_2$ (admitiendo valores negativos ante desvíos largos) y mermas en carga agrícola.
4. **Orquestador Demo WFEO (`E2E-01`):** Simulación integral de emergencia en Carpish con ruteo multimodal (ambulancia vs. camión pesado) en menos de 50 ms.
5. **Servidor HTTP REST (`/api/v1`):** Endpoints nativos reactivos para recálculo de $IVV$, mapas GeoJSON y gestión de simulaciones.
6. **Interfaz CLI JSON (`stdin` / `stdout`):** Invocación directa del motor mediante tuberías JSON para integración modular.

---

## Requisitos y Ejecución

* **Java 21 / 25**
* **Maven 3.9+**

### Interfaz Gráfica de Usuario (100% Java Swing / AWT)
```bash
java -cp "target/classes:$(mvn dependency:build-classpath -q | tail -n 1)" pe.qullaruta.gui.WfeoDemoGui
```

### Iniciar Servidor HTTP REST (100% Java Puro)
```bash
java -cp "target/classes:$(mvn dependency:build-classpath -q | tail -n 1)" pe.qullaruta.server.TrackCHttpServer
```

### Ejecutar Suite de Pruebas (DO-178C / BVA / Invariantes)
```bash
mvn clean test
```

### Ejecutar Simulación Demo WFEO en Consola
```bash
java -cp target/classes pe.qullaruta.simulator.WfeoDemoRunner
```

### Ejecutar Interfaz CLI JSON
```bash
echo '{"command":"RUN_WFEO_DEMO"}' | java -cp "target/classes:$(mvn dependency:build-classpath -q | tail -n 1)" pe.qullaruta.cli.TrackCCli
```
