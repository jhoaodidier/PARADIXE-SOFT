# QULLA-RUTA · Prototipo de presentación en Java

Proyecto Java con Maven para la primera presentación del curso de Java Machine Learning en Apache NetBeans. La demo muestra el cálculo determinista del Índice de Vulnerabilidad Vial (IVV) y la simulación de un escenario en Carpish con datos sintéticos.

## Abrir y ejecutar en Apache NetBeans

1. En **File → Open Project**, abre la carpeta `prototipo-java-ml`, que contiene `pom.xml`. Si el proyecto ya estaba abierto antes del cambio en el POM, ciérralo y vuelve a abrirlo.
2. En **Projects**, haz clic derecho sobre `qulla-ruta-core` y selecciona **Run Project**. Se abrirá la ventana **Qulla-Ruta · Carpish** con los resultados del escenario sintético.
3. Para presentar la demo, recorre el corredor vial, el índice de vulnerabilidad, la respuesta por vehículo y el impacto en la carga. **Repetir simulación** vuelve a ejecutar el escenario con los mismos datos y genera un nuevo identificador.

## Módulos Implementados (Track C: Motor Geotécnico IVV & Simulador WFEO)

1. **Motor de Vulnerabilidad Vial ($IVV$):** Cálculo determinista y reproducible cada 3 horas clasificando tramos en `GREEN`, `AMBER` y `RED` según lluvia acumulada 24/72h, pendientes, quebradas, tipo de suelo y puntos críticos históricos.
2. **Notification Firewall:** Aislamiento estricto de seguridad que intercepta y aborta cualquier despacho a pasarelas reales (SMS/WhatsApp) en contexto `SIMULATION`.
3. **Métricas de Impacto con Signo Libre ($\Delta \in \mathbb{R}$):** Cálculo fiel de $\Delta T$, $\Delta \text{CO}_2$ (admitiendo valores negativos ante desvíos largos) y mermas en carga agrícola.
4. **Orquestador Demo WFEO (`E2E-01`):** Simula una emergencia en Carpish y muestra las opciones para una ambulancia 4×4 y un camión pesado.
5. **Servidor HTTP REST (`/api/v1`):** Endpoints nativos reactivos para recálculo de $IVV$, mapas GeoJSON y gestión de simulaciones.
6. **Interfaz CLI JSON (`stdin` / `stdout`):** Invocación directa del motor mediante tuberías JSON para integración modular.

---

## Requisitos y Ejecución

* **Java 21 / 25**
* **Maven 3.9+**

Ejecuta estos comandos desde la carpeta del prototipo.

### Ejecutar Suite de Pruebas (DO-178C / BVA / Invariantes)
```bash
mvn clean test
```

### Ejecutar Simulación Demo WFEO en Consola
```bash
mvn compile
java -cp target/classes pe.qullaruta.simulator.WfeoDemoRunner
```

### Ejecutar Interfaz CLI JSON
```bash
mvn -q compile dependency:build-classpath -Dmdep.outputFile=target/classpath.txt
printf '%s\n' '{"command":"RUN_WFEO_DEMO"}' | java -cp "target/classes:$(cat target/classpath.txt)" pe.qullaruta.cli.TrackCCli
```
