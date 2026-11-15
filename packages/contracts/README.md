# @rumbo/contracts

Contratos TypeScript compartidos entre el host, los adaptadores y la UI.

- `Result<T, E>` expresa éxito con `value` y fallo con `error.code`. Úsalo con
  `ok(value)` y `err({ code })`. `ERROR_CATALOG` define el nivel de los errores traducibles.
- `SIM_EDGE_ERROR_CODES` se compara con los códigos de Rust en la prueba de paridad.
  Los rechazos de WASM conservan el formato `<CODE>: <detalle>`. El protocolo del worker es v2.
- El host crea `createLogger` por sesión e inyecta el `LogPort`. El historial conserva los registros
  en orden cronológico. En publicación la consola muestra desde `warn` y el historial omite `trace`.
- `createCounterRing` conserva muestras de rendimiento tomadas a 1 Hz por el host.
  `formatDiagnostic` reúne ambas historias cuando el jugador solicita copiar el diagnóstico.
  La copia incluye el navegador y su versión, los códigos de registro y un conjunto de campos
  conocidos. Los mensajes de ejecución ajenos a la lista permitida se marcan como redactados.
