# Rumbo — spikes del PoC

Spikes del PoC, en orden de riesgo:

- **S0 · Laboratorio de manejo:** un auto, una pista hecha a mano y un panel para calibrar el manejo en vivo.
  Responde: ¿el manejo arcade engancha lo suficiente como para seguir dando vueltas?
- **S1 · Datos geográficos:** extracto de OpenStreetMap, proyección al plano local, grafo vial y ruteo A→B.
  Responde: ¿la red vial de Lima es ruteable y limpia? Ver [S1](#s1--datos-geográficos).

Todavía no hay elevación real ni intersecciones conducibles. Eso viene en S2 y S3.

## Requisitos

- Rust estable con el target de WebAssembly: `rustup target add wasm32-unknown-unknown`
- [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/): `cargo install wasm-pack`
- Node 20 o superior

## Arrancar

```bash
cd apps/web
npm install
npm run dev        # compila el WASM en release y abre Vite
```

Si solo cambiaste código de TypeScript, `npm run dev:web` arranca sin recompilar el WASM.

## Qué hay

```
crates/
  sim-core/   Simulación determinista, sin nada de navegador
    src/track.rs       Mini compilador de pistas: eje, coordenada s, superficies, mallas, muros
    src/vehicle.rs     Auto: suspensión por raycast, neumáticos arcade, capa de asistencias
    src/profile.rs     Parámetros de manejo con unidades (una sola definición: struct + esquema del panel)
    src/race.rs        Vueltas y checkpoints sobre la coordenada s
    src/replay.rs      Grabación RLE de entradas, manifest y verificación
    src/simulation.rs  Sesión completa y hash canónico del estado
    src/autopilot.rs   Piloto automático (prueba de manejo automática y replays de referencia)
    tests/             Determinismo, replays dorados, casos adversos y pruebas de manejo
  sim-wasm/   Capa fina de wasm-bindgen; no tiene lógica de simulación
apps/web/     Vite + React + three.js (WebGPU, con WebGL 2 de respaldo)
  src/workers/sim.worker.ts   Dueño del reloj de 120 Hz
  src/render/                 Render: solo lee estado, nunca escribe en la simulación
  src/debug/                  Panel de ajuste, telemetría y sesión
assets/
  tracks/test-loop.json       Pista de control: recta, horquilla, eses y curva de derrape
  profiles/hatch-arcade.json  Perfil por defecto
scripts/verify-golden.mjs     Verifica los replays dorados con el WASM en Node
```

## Controles

| Tecla | Acción |
| --- | --- |
| W / ↑ | Acelerar |
| S / ↓ | Frenar; marcha atrás si estás detenido |
| A D / ← → | Dirección |
| Espacio + dirección | Derrape (sobre 55 km/h, con dirección marcada) |
| S + dirección | Frenada en curva, sin derrape |
| R | Recolocar el auto sobre la pista |
| P | Piloto automático |
| C | Cambiar cámara (persecución, cercana, aérea) |
| G | Fantasma de la mejor vuelta |
| H | Mostrar u ocultar el panel |
| Esc | Pausa |

Con mando estándar: gatillos para acelerar y frenar, stick izquierdo para la dirección, A o RB freno de mano, Y recolocar.

## Cómo calibrar

1. Ajusta parámetros en la pestaña **Ajustes**. Los que cambiaste se marcan con una barra ámbar.
2. **Aplicar** empieza una sesión nueva con el perfil congelado: el auto vuelve a la grilla y el replay arranca de cero.
   Así cada intento es reproducible.
3. Corre contra el **fantasma** de tu mejor vuelta. El delta del HUD te dice si el cambio te hizo más rápido.
4. El indicador semicircular muestra el ángulo de derrape; la banda ámbar es el objetivo del perfil.
5. **Exportar perfil** guarda el JSON; puedes versionarlo en `assets/profiles/`.

Las tres pruebas de manejo sugeridas están todas en la pista de control:
recta y frenada a la horquilla, curvas enlazadas en las eses, y derrape en la curva amplia con peralte.

## Pruebas

```bash
cargo test -p sim-core --release
```

Incluyen:

- **Determinismo:** mismas entradas dan el mismo hash; una entrada alterada se detecta en el siguiente control (cada 600 ticks).
- **Replay dorado:** `crates/sim-core/tests/golden/autopilot-test-loop.json` se reproduce bit a bit.
  Regenéralo con `UPDATE_GOLDEN=1 cargo test -p sim-core --release golden` cuando cambie la física
  (y sube `PHYSICS_VERSION` en `sim-core/src/lib.rs`).
- **Casos adversos:** choques a fondo, volante al tope y resets repetidos; el estado nunca deja de ser finito.
- **Prueba de manejo automática:** el piloto automático da varias vueltas; falla ante golpes verticales,
  ruedas en el aire o contacto con el muro.
- **Manejo:** reposo sobre cuatro ruedas, 0-100 km/h en un rango plausible, frenada en curva sin derrape,
  derrape controlado con freno de mano (y por toque de freno si el perfil lo activa) y roce de muro sin salir despedido.

### Nativo contra WASM

```bash
cd apps/web
npm run verify-golden
```

Corre el mismo replay dorado con el binario WASM en Node. Si coincide, nativo y WASM son deterministas entre sí.
En el navegador, **Sesión → Verificar esta sesión** re-simula todo lo que manejaste y compara hashes.

`cargo run -p sim-core --release --example drift_entry` compara frenar en curva contra derrapar con freno de mano.
`cargo run -p sim-core --release --example probe` imprime telemetría de escenarios básicos (reposo, aceleración,
derrape, choque y piloto automático), útil al tocar la física.

## S1 · Datos geográficos

El crate `crates/track-compiler` lleva la red vial real hasta un recorrido resuelto, que es la entrada de S2.

```
data/queries/lima-costa-verde.overpassql   Consulta: Miraflores, Barranco, parte de San Isidro y la Costa Verde
scripts/fetch-osm.mjs                      Descarga el extracto y lo archiva con fecha en data/raw/
crates/track-compiler/src/
  osm.rs          Lectura de Overpass JSON
  geo.rs          WGS84 → plano tangente ENU en metros (f64, libm)
  graph.rs        Grafo dirigido: se parte solo en nodos compartidos; atributos y anchos estimados
  routing.rs      Ajuste a la calle y A* determinista entre puntos de paso
  diagnostics.rs  Severidad (info, advertencia, error), independiente del estado de la pista
  export.rs       GeoJSON (orden [lon, lat]) del grafo y de la ruta
```

### Cerrar S1 con datos reales

```bash
# 1. Descargar el extracto (si Overpass responde 429 o 504, espera o usa otro servidor con OVERPASS_URL)
node scripts/fetch-osm.mjs

# 2. Construir el grafo y su GeoJSON
cargo run -p track-compiler --release -- ingest data/raw/lima-costa-verde-AAAA-MM-DD.overpass.json \
  --region lima-costa-verde --out data/graphs/lima-costa-verde.json --geojson data/graphs/lima-costa-verde.geojson

# 3. Rutear entre dos puntos (lat,lon; cópialos con clic derecho en openstreetmap.org)
cargo run -p track-compiler --release -- route data/graphs/lima-costa-verde.json \
  --from LAT,LON --to LAT,LON --geojson ruta.geojson --descriptor ruta.descriptor.json

# 4. Pruebas sobre el extracto real (se activan solas si hay un archivo en data/raw/)
cargo test -p track-compiler --release
```

Para verificar contra el mapa, arrastra los `.geojson` a [geojson.io](https://geojson.io): el grafo debería calzar
con las calles y la ruta debería seguirlas sin atajos por veredas ni manzanas.

Los criterios de salida de S1 son:

- [ ] El extracto descarga, se archiva y el grafo se construye sin errores.
- [ ] La mayor componente conexa tiene más del 85 % de los nodos (lo comprueba la prueba real).
- [ ] Varias rutas A→B en la Costa Verde siguen las calles al revisarlas en geojson.io, incluidas las bajadas.
- [ ] Los pasos a desnivel no aparecen como cruces: no hay giros entre una vía y el puente que la cruza.
- [ ] Revisar los diagnósticos: cuántas vías no tienen ancho ni carriles en OSM y si eso cambia la estimación.

Algunas notas:

- **Sentidos de circulación:** por defecto el ruteo los ignora, como propone el documento para carreras.
  `--respect-oneway` los respeta (modo Exploración). `--closed` cierra el circuito volviendo al primer punto.
- **Orden de coordenadas:** siempre `lat,lon`. Si un punto cae lejos de la red pero invertido sí cae dentro,
  el error lo dice. El rango por sí solo no basta: la longitud de Lima (−77) cabe como latitud.
- **Snapshots:** versiona en git el extracto con el que valides S1, porque el hash del grafo depende de él.
  Si supera unos 20 MB, usa Git LFS.
- **Licencia:** los datos son © colaboradores de OpenStreetMap, bajo licencia
  [ODbL](https://www.openstreetmap.org/copyright). Cualquier pantalla que los muestre debe atribuirlos.

## Decisiones tomadas en S0

- **Rapier 0.36 para cuerpo rígido y colisiones; vehículo propio.** Con `enhanced-determinism`, Rapier fuerza
  matemática escalar y `libm` en glam. La trigonometría propia también pasa por `libm`.
- **120 Hz** como frecuencia de simulación; el render interpola a la frecuencia del monitor.
- **Muros con cajas sólidas para colisión.** Una malla delgada dejaba que el auto penetrara y saliera empujado
  hacia arriba por la tapa del muro. La malla del muro queda solo para el render.
- **Muro indulgente:** mientras el chasis toca un muro, se recorta la velocidad vertical hacia arriba y se amortiguan
  cabeceo y balanceo. Rozar un muro desvía el auto, no lo lanza.
- **Derrape con freno de mano**, como en los NFS clásicos: el freno normal frena en curva. La entrada por toque de
  freno sigue disponible como opción del perfil (`drift_brake_tap`).
- **Derrape con control de velocidad de giro**, no de ángulo directo: el ángulo converge sin oscilar.
- **Los rayos de rueda ignoran los muros** y las normales casi verticales.
- **`postMessage` entre worker y render**, sin `SharedArrayBuffer`: el estado de un auto pesa unos cientos de bytes.

## Pendiente para S0

- Verificar nativo contra WASM en tu máquina (`npm run verify-golden`) y en Chrome, Firefox y Safari.
- Medir 60 Hz con subpasos contra 120 Hz y 240 Hz, como pide el documento de arquitectura.
- Calibrar el perfil con teclado y con mando hasta que la respuesta a la pregunta de S0 sea clara.
