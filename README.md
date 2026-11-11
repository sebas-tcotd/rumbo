# Rumbo — spikes del PoC

Spikes del PoC, en orden de riesgo:

- **S0 · Laboratorio de manejo:** un auto, una pista hecha a mano y un panel para calibrar el manejo en vivo.
  Responde: ¿el manejo arcade engancha lo suficiente como para seguir dando vueltas?
- **S1 · Datos geográficos:** extracto de OpenStreetMap, proyección al plano local, grafo vial y ruteo A→B.
  Responde: ¿la red vial de Lima es ruteable y limpia? Ver [S1](#s1--datos-geográficos).

- **S2 · Primer tramo real (First Drivable):** del recorrido resuelto a una pista conducible con elevación.
  Responde: ¿se maneja continuo sobre datos reales? Ver [S2](#s2--primer-tramo-real).

- **S3 · Intersecciones:** cruces en T y de cuatro brazos conducibles, con calles laterales.
  Responde: ¿cuánto cuesta de verdad una intersección? Ver [S3](#s3--intersecciones).
- **S4 · Determinismo:** los mismos replays dan los mismos hashes en nativo, Node y cinco navegadores.
  Responde: ¿se puede confiar un tiempo entre jugadores? Ver [S4](#s4--determinismo).

**El PoC está cerrado** (ver [Estado del PoC](#estado-del-poc)). Compartir (S5) y la identidad visual (S6)
pasan al desarrollo formal con BMAD.

## Requisitos

Cada herramienta que toca el build tiene versión exacta, la misma en local y en CI.

- **Rust:** solo hace falta [rustup](https://rustup.rs/). La versión (1.98.1), el target
  `wasm32-unknown-unknown` y los componentes `rustfmt` y `clippy` salen de `rust-toolchain.toml`;
  `rustup toolchain install` en la raíz los instala (o se instalan solos con el primer `cargo`).
- **Node 24:** la versión exacta está en `.nvmrc` (`nvm use` / `fnm use`; con Volta,
  `volta install node@$(cat .nvmrc)`).
- **wasm-pack 0.15.1** exacto: `cargo install wasm-pack --version 0.15.1 --locked`. Va de la mano de
  `wasm-bindgen` 0.2.130 (wasm-pack descarga el `wasm-bindgen-cli` que coincide con `Cargo.lock`).
- **pnpm 11.28.11** exacto (`packageManager` en el `package.json` raíz): instálalo con el instalador oficial o
  `pnpm self-update 11.28.11`. No usamos Corepack.

Las dependencias directas de npm y de Cargo van en versión exacta (`x.y.z` / `=x.y.z`), y
`node ci/exact-versions/check.mjs` falla en CI si alguna usa un rango. Dependabot propone una subida por
PR y nada se mergea solo.

## Navegadores soportados

En escritorio (móvil y táctil quedan fuera del MVP):

| Navegador | Versión mínima |
|---|---|
| Chrome / Edge | **103** |
| Firefox | **114** |
| Safari | **16.4** |

Cada mínimo lo fija la API más nueva que el juego necesita (datos de
[MDN browser-compat-data](https://github.com/mdn/browser-compat-data) 8.1.8, consultados el 2026-11-05):

| API | Chrome / Edge | Firefox | Safari | Fija el mínimo de |
|---|---|---|---|---|
| WebGL 2 | 56 / 79 | 51 | 15 | — |
| `CompressionStream` / `DecompressionStream` con `'deflate-raw'` | **103** | 113 | **16.4** | Chrome/Edge, Safari |
| CSP `'wasm-unsafe-eval'` | 97 | 102 | 16 | — |
| Workers de módulo (`new Worker(url, { type: 'module' })`) | 80 | **114** | 15 | Firefox |
| Gamepad API | 21 / 12 | 29 | 10.1 | — (opcional: sin mando se juega con teclado) |

- El juego detecta **funciones, no el user agent**: si falta una de las APIs obligatorias muestra una
  pantalla de navegador no soportado en vez de fallar a medias (llega con la historia 0.8).
- WebGPU es opcional: se usa si está disponible y, si no, el render cae a WebGL 2.
- En las versiones actuales de cada navegador se verifica el determinismo en CI (Chromium, Firefox, WebKit
  y Safari real; ver [S4](#s4--determinismo)).

## Arrancar

```bash
pnpm install
pnpm turbo run build                 # compila el WASM (cacheado por Turbo), los paquetes y la app (dist/ y dist-lab/)
pnpm turbo run test                  # cargo test --release --workspace + dorados en Node + exact-versions
pnpm --filter @rumbo/web dev         # compila WASM y paquetes (predev, cacheado) e inicia Vite con devtools
```

## Qué hay

```
crates/
  sim-core/   Simulación determinista, sin nada de navegador
    src/track.rs       Mini compilador de pistas: eje, coordenada s, superficies, mallas, muros
    src/vehicle.rs     Auto: suspensión por raycast, neumáticos arcade, capa de asistencias
    src/profile.rs     Reexports del perfil compartido en track-format/domain/profile.rs
    src/race.rs        Vueltas y checkpoints sobre la coordenada s
    src/replay.rs      Grabación RLE y verificación; DTO en track-format/domain/replay.rs
    src/simulation.rs  Sesión completa y hash canónico del estado
    src/autopilot.rs   Piloto automático (prueba de manejo automática y replays de referencia)
    tests/             Determinismo, replays dorados, casos adversos y pruebas de manejo
  sim-wasm/   Capa fina de wasm-bindgen; no tiene lógica de simulación
packages/contracts/     Puertos, protocolo versionado, layout de 41 campos f32, ajustes y tiempo de frame
packages/sim-worker/    Worker, binding WASM, reloj fijo y cliente de SimPort
packages/render-three/  ThreeRenderer (RendererPort): three.js WebGPU con WebGL 2 de respaldo; solo lee estado
packages/platform-web/  Adapters del navegador: input/, storage/ (rumbo.settings.v1), audio/, telemetry/, zone-source/
packages/ui/            HUD, carga, avisos y fatal provisional; React + react-i18next (recurso es)
packages/devtools/      Laboratorio: paneles de ajuste, telemetría y sesión (solo en builds con devtools)
apps/web/               @rumbo/web: composition root único (src/main.tsx), bucle de frame y store del lab
tools/data-fetch/       fetch-osm.mjs: descarga de extractos de OpenStreetMap
assets/
  tracks/test-loop.json       Pista de control: recta, horquilla, eses y curva de derrape
  profiles/hatch-arcade.json  Perfil por defecto
```

## Worker de simulación

El worker ejecuta hasta ocho ticks de 1/120 s por despertar y conserva la fracción restante. Una pausa manual y una pestaña oculta suspenden el reloj. El laboratorio conserva la entrada por tick y el formato `race-lab-replay/1`.

Tres slots de 328 bytes transportan los estados anterior y actual (41 f32 cada uno). El mensaje lleva la versión del layout, generación, slot y timestamp. El cliente copia ambos estados a memoria propia antes de devolver el buffer mediante transferencia. El renderer puede leer hasta la próxima entrega. React conserva una copia a unos 15 Hz. Una vuelta completada y sus frames de fantasma viajan por un mensaje independiente del pool.

`hashContent` usa BLAKE3-128 de Rust. La codificación de retos con Race, Replay y RouteMeta y la carga de fantasmas externos devuelven `UNSUPPORTED_CAPABILITY`. Las fixtures prospectivas comprueban la cabecera de replay v2 y el prefijo de link inline. La semilla se expresa como decimal u64 en la fixture JSON. La escritura de ambos formatos corresponde a 0.10 y 6.7.

`pnpm turbo run build` compila los paquetes. `pnpm turbo run test --force` ejecuta paridad, transferencias y negativos de capas. Para revisar Biome usa `pnpm exec biome lint` (paquetes y `apps/`). Con Chromium instalado en Playwright, `node ci/determinism/lab-smoke.mjs` prueba los bundles (laboratorio en `dist-lab/` y publicación en `dist/`) y `node ci/determinism/lab-smoke.mjs --dev` prueba Vite. `node ci/bundle/check-devtools.mjs` comprueba que `@rumbo/devtools` no está en `dist/` y sí en `dist-lab/`.

## Front: paquetes y composition root

`apps/web/src/main.tsx` es el único composition root (ADR-001): crea `createWorkerSim`, `createThreeRenderer`,
`createWebPlatform`, el store del lab y `mountUi`, y les pasa sus dependencias por argumento. Ningún paquete
instancia a otro adapter ni hay singletons; con HMR, `import.meta.hot.dispose` desarma todo antes de volver a armar.

- **Render:** un solo `requestAnimationFrame` en `apps/web/src/frame-loop.ts` muestrea la entrada, calcula
  `alpha` con `interpolationAlpha` (`@rumbo/contracts`, tiempo absoluto) y llama
  `RendererPort.frame(prev, curr, alpha)`. El renderer no tiene bucle propio ni retiene `StateMsg`.
- **Capas:** cada paquete tiene su regla `noRestrictedImports` en `biome.json`, generada desde
  `ci/layers/policy.mjs` (ADR-005; ver «Capas»). `ui` y `devtools` nunca importan three ni otros adapters.
- **Exports:** los paquetes que consume Vite (`sim-worker`, `render-three`, `platform-web`, `ui`, `devtools`)
  exportan la fuente (`"default": "./index.ts"`) y los tipos desde `dist/`, generados por `tsc -b`
  (`emitDeclarationOnly` en los paquetes nuevos). `@rumbo/contracts` sigue emitiendo JS porque lo importan
  pruebas de Node. Los imports relativos llevan extensión `.ts`/`.tsx` para que Node (type stripping) ejecute
  las pruebas co-localizadas sin compilar.
- **Ajustes (`rumbo.settings.v1`):** esquema versionado en `packages/platform-web/storage/` con migraciones
  puras encadenadas. **Toda historia que guarde un ajuste lo agrega a ese esquema** (campo en `Settings`,
  `SETTINGS_VERSION` + 1, migración `vN → vN+1`, validación y fixture); nunca una clave nueva de
  `localStorage`. La UI solo usa `SettingsStorePort`. Detalle en `packages/platform-web/README.md`.
- **i18n:** `mountUi` crea su propia instancia de i18next (`createInstance`, `initAsync: false`) con el
  recurso `packages/ui/i18n/es.json`; claves `pantalla.elemento` en camelCase. Los avisos llegan como código
  (`errors.<CODIGO>`). Los textos de devtools quedan literales: es una herramienta de desarrollo.
- **Devtools:** flag de build. Activo en `vite dev`, en `vite build --mode lab` (`apps/web/dist-lab/`) y con
  `RUMBO_DEVTOOLS=1`; apagado en la publicación (`apps/web/dist/`), donde el `import('@rumbo/devtools')` queda
  detrás de una constante `false` y no se emite el chunk. `pnpm --filter @rumbo/web build` genera ambas salidas.

## Kernel compartido

`track-format` contiene los DTO de pista, replay v1, entrada y perfil, sin dependencias de
simulación ni I/O. `sim-core` conserva los reexports de datos; las operaciones de runtime
son `sim_core::compile_track(&TrackDef)` y `sim_core::verify_replay(&ReplayManifest, &TrackDef)`.

FNV-1a tiene un solo acumulador, con texto prefijado por `u32` para simulación y por `u64`
para compilador. Se preservan los hashes legados. La API nueva `codec::{encode, decode}` usa
`RTF\0` + versión `u16` little-endian + postcard 1.1.3; el consumidor debe conocer el tipo DTO.
`hash::content_hash` identifica bytes con los primeros 16 bytes de BLAKE3 (32 dígitos hex).
El sobre binario es independiente de los lectores JSON y rechaza versiones desconocidas y bytes sobrantes.

`rng::Pcg32` implementa XSH-RR 64/32: siembra de referencia, `initstate=seed`,
`initseq=id`; ids estables: bots=1, start=2, rules=3 (63 bits). Los streams se crean por separado
con `for_subsystem`; consumir uno no avanza los demás. Todavía no se conectan al tick.
El RNG de decoración sigue separado. El replay legado conserva 120 Hz y las versiones existentes.

`NetworkDef` conserva vértices 3D y su pertenencia a triángulos: admite alturas distintas sobre
el mismo (x,z). Esto prepara los datos; la conducción por pasos a desnivel queda pendiente de D11.

Controles del kernel (incluidos casos negativos de capas y serde):

```bash
node ci/track-format/check.mjs
node --test ci/track-format/check.test.mjs
cargo test --release -p track-format
cargo check -p track-format --target wasm32-unknown-unknown
cargo clippy -p track-format --all-targets -- -D warnings -A clippy::pedantic
```

La prueba `per_tick` compara huellas de todos los ticks de los ocho replays con las capturadas
antes de la extracción en `18649c3`, además de los controles y hashes finales dorados.

## Capas (ADR-005)

Toda regla de capas tiene una herramienta que la hace cumplir y un test que prueba que sigue viva
(una violación intencional tiene que fallar). La fuente única es `ci/layers/policy.mjs`.

- **TypeScript:** dependencias `workspace:*` y project references (`tsc -b`, `^build` en Turbo).
  `biome.json` se **genera** desde la política (`node ci/layers/biome.mjs`; no se edita a mano):
  `noRestrictedImports` por paquete y por profundidad de carpeta, subcarpetas de `platform-web`
  aisladas entre sí y de la raíz del paquete, y `apps/web` (composition root) que importa los paquetes
  por nombre, nunca por ruta relativa. Cada mensaje dice qué usar y cita el ADR.
- **Rust:** la dirección de dependencias la impone Cargo y la verifica el meta-check. `[workspace.lints]`
  hace error de `unwrap_used`, `expect_used` (fuera de tests) y de todo `#[allow]` sin `reason`;
  `pedantic` es warning. Los crates del núcleo (`track-format`, `sim-core`, `track-compiler-core`)
  tienen un `clippy.toml` (copia de `ci/layers/core-clippy.toml`) que prohíbe `HashMap`/`HashSet`,
  reloj, sistema de archivos, red y las funciones de float no deterministas (usar `libm`) (P1).
- **Meta-check:** `node ci/layers/check.mjs` falla si un paquete de pnpm o un miembro de Cargo no está en
  la política, si sus `@rumbo/*` o sus project references no coinciden con lo permitido, si un crate no
  hereda `[lints] workspace = true`, si un `clippy.toml` del núcleo difiere del canónico o si
  `biome.json` no está al día.

**Agregar un paquete o crate:** en el mismo PR, su entrada en `ci/layers/policy.mjs` (lo que puede importar
y sus casos `allowed`/`rejected`), `node ci/layers/biome.mjs`, su project reference y, si es crate,
`[lints] workspace = true` (y el `clippy.toml` canónico si es del núcleo). Detalle en `ci/layers/README.md`.

```bash
node ci/layers/check.mjs
pnpm exec biome lint
pnpm --filter rumbo-layers test             # meta-check, generador y guardrail de Biome
cargo clippy --workspace --all-targets --locked -- -D warnings -A clippy::pedantic
pnpm --filter rumbo-layers run test:clippy  # guardrail de Clippy (lento)
```

El job `layers` de `.github/workflows/ci.yml` corre todo lo anterior y publica el conteo de avisos
`pedantic` en el resumen del run.

## Controles

| Tecla | Acción |
| --- | --- |
| W / ↑ | Acelerar |
| S / ↓ | Frenar; marcha atrás si estás detenido |
| A D / ← → | Dirección |
| Espacio + dirección | Derrape (sobre 55 km/h, con dirección marcada) |
| S + dirección | Frenada en curva, sin derrape |
| R | Recolocar el auto sobre la pista |
| T | Volver a la salida y empezar otro intento (en sprints, otra pasada) |
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
- **Replays dorados:** los 8 de `crates/sim-core/tests/golden/` (ver [S4](#s4--determinismo)) se reproducen
  bit a bit. Regenéralos con `UPDATE_GOLDEN=1 cargo test -p sim-core --release --test golden` cuando cambie la
  física (y sube `PHYSICS_VERSION` en `track-format/src/version.rs`).
- **Casos adversos:** choques a fondo, volante al tope y resets repetidos; el estado nunca deja de ser finito.
- **Prueba de manejo automática:** el piloto automático da varias vueltas; falla ante golpes verticales,
  ruedas en el aire o contacto con el muro.
- **Manejo:** reposo sobre cuatro ruedas, 0-100 km/h en un rango plausible, frenada en curva sin derrape,
  derrape controlado con freno de mano (y por toque de freno si el perfil lo activa) y roce de muro sin salir despedido.

### Nativo contra WASM

```bash
pnpm turbo run test --filter rumbo-determinism
```

Corre los replays dorados con el binario WASM en Node. Si coincide, nativo y WASM son deterministas entre sí.
En el navegador, **Sesión → Verificar esta sesión** re-simula todo lo que manejaste y compara hashes.

`cargo run -p sim-core --release --example drift_entry` compara frenar en curva contra derrapar con freno de mano.
`cargo run -p sim-core --release --example probe` imprime telemetría de escenarios básicos (reposo, aceleración,
derrape, choque y piloto automático), útil al tocar la física.

## S1 · Datos geográficos

El CLI `compiler-cli` lee Overpass y GeoTIFF y escribe los archivos. El núcleo puro `crates/track-compiler-core` lleva la red vial real hasta un recorrido resuelto, que es la entrada de S2.

```
data/queries/lima-costa-verde.overpassql   Consulta: Miraflores, Barranco, parte de San Isidro y la Costa Verde
tools/data-fetch/fetch-osm.mjs             Descarga el extracto y lo archiva con fecha en data/raw/
crates/track-compiler-core/src/
  osm.rs          Lectura de Overpass JSON
  geo.rs          WGS84 → plano tangente ENU en metros (f64, libm)
  graph.rs        Grafo dirigido: se parte solo en nodos compartidos; atributos y anchos estimados
  routing.rs      Ajuste a la calle y A* determinista entre puntos de paso
  diagnostics.rs  Severidad (info, advertencia, error), independiente del estado de la pista
  export.rs       GeoJSON (orden [lon, lat]) del grafo y de la ruta
```

El núcleo comparte las identidades de `track-format` y usa serde, serde_json y libm para el cálculo en memoria. TIFF y los archivos pertenecen a `compiler-cli`. CI compila el núcleo para wasm32 y comprueba dependencias normales y de build. El CLI puede usar el simulador en pruebas. CI rechaza dependencias normales o de build que lo conecten con el simulador, Rapier o los bindings del navegador. El detector de tokens Rust no expande macros.

### Cerrar S1 con datos reales

```bash
# 1. Descargar el extracto (si Overpass responde 429 o 504, espera o usa otro servidor con OVERPASS_URL)
node tools/data-fetch/fetch-osm.mjs

# 2. Construir el grafo y su GeoJSON
cargo run -p compiler-cli --release -- ingest data/raw/lima-costa-verde-AAAA-MM-DD.overpass.json \
  --region lima-costa-verde --out data/graphs/lima-costa-verde.json --geojson data/graphs/lima-costa-verde.geojson

# 3. Rutear entre dos puntos (lat,lon; cópialos con clic derecho en openstreetmap.org)
cargo run -p compiler-cli --release -- route data/graphs/lima-costa-verde.json \
  --from LAT,LON --to LAT,LON --geojson ruta.geojson --descriptor ruta.descriptor.json

# 4. Pruebas sobre el extracto real (se activan solas si hay un archivo en data/raw/)
cargo test -p compiler-cli -p track-compiler-core --release
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

## S2 · Primer tramo real

`rumbo-compiler track` convierte una ruta en una pista de `sim-core`: eje suavizado, ancho por calle, elevación
filtrada desde Copernicus GLO-30 y un estado (`VALID`, `DEGRADED`, `UNSUPPORTED`) con diagnósticos localizados.

```bash
cargo run -p compiler-cli --release -- track data/graphs/lima-costa-verde.json \
  --dem data/raw/Copernicus_DSM_COG_10_S13_00_W078_00_DEM.tif \
  --from -12.100321,-77.069759 --to -12.1596,-77.0256 --respect-oneway \
  --anchor -12.109,-77.057,6 --anchor -12.113,-77.051,6 \
  --anchor -12.149711,-77.024503,4.2 --anchor -12.1527,-77.0245,4.5 --anchor -12.154993,-77.025104,4.3 \
  --id costa-verde-sprint --name "Costa Verde · Circuito de Playas" \
  --out assets/tracks/costa-verde-sprint.json --profile perfil.csv
```

En el laboratorio, elige la pista en **Sesión → Pista activa** (o abre `?pista=costa-verde-sprint`).
Es un sprint: el tiempo corre de la salida a la llegada, y **T** vuelve a la salida.

`perfil.csv` trae, por cada muestra, la altura del modelo en el eje, la tomada a lo ancho, la de la pista y cuánto
varía el terreno a lo ancho. Ábrelo en una planilla y grafícalo para ver dónde el modelo engaña.

### Qué hace con los datos

- **Ruteo con límite de giro:** A* sobre aristas dirigidas; ningún cruce con giros de más de 135° (`--max-turn`)
  y un costo por giro (`--turn-cost`). Sin esto, la ruta entraba a las rampas con medias vueltas de 168°.
- **Eje:** suavizado gaussiano de 4 m y re-muestreo cada 2 m. El ancho sale de la calle (acotado a 7-16 m) y se
  suaviza para no tener escalones.
- **Elevación:** percentil bajo a lo ancho de la calzada, puentes y túneles interpolados entre sus extremos,
  mediana móvil de 120 m, pendiente máxima del 14 % y suavizado.
- **Anclas:** `--anchor lat,lon,altura` (repetible) fija la altura de la vía donde la conoces. Un ancla sola corrige
  su entorno y la corrección se desvanece a 400 m. Entre dos anclas a menos de 800 m, la vía queda en línea recta
  de una a otra: así se dice "este tramo es plano". Para aplanar una loma falsa, pon un ancla en cada borde (con la
  altura que ya tiene ahí) y otra en medio. Quedan registradas en `source.elevationAnchors` de la pista.
- **Sentido de las calles:** si el recorrido va contra el sentido de una calle de un solo sentido, la pista lleva
  el aviso `against-oneway`. En una vía de calzadas separadas eso suele significar que va por la calzada del otro
  lado, que puede ir a otra altura. Usa `--respect-oneway`.

### Lo que aprendimos

- **First Drivable pasa:** el piloto automático recorre los 8,9 km en 3 min 27 s, hasta 182 km/h, con un golpe
  vertical máximo de 0,06 m/s (desde S3 frena más en las curvas cortas y más tarde en todas), sin ruedas en el aire ni contacto con muros (`sim-core/tests/first_drivable.rs`).
- **El modelo de elevación es el punto débil.** GLO-30 es un modelo de *superficie* (incluye edificios y árboles),
  difumina el acantilado en unos 100 m y se tomó entre 2011 y 2015. Junto al acantilado mezcla la vía con la
  ladera, y por eso la pista queda `DEGRADED`, con avisos en cada tramo con pared al costado. Para el producto hará
  falta un modelo de *terreno* (sin edificios) o anclas puestas a mano.
- **La loma de 41 m del km 2 era, sobre todo, la calzada equivocada.** La primera versión iba de norte a sur por
  la calzada que va de sur a norte, que en el km 2 sube por la ladera. Con `--respect-oneway` la pista va por la
  calzada de abajo. Lo confirmamos en Google Maps 3D: cerca de -12.109,-77.057 una calzada sube y la otra sigue
  abajo, y en -12.113,-77.051 las dos están al mismo nivel. Ahí quedaba una loma de 20 m que el modelo sacaba de la
  ladera, así que pusimos dos anclas a 6 m (el nivel de los tramos planos vecinos).
- **En Los Yuyos (km 8,1) el modelo marcaba una loma de 10 m** donde la vía es plana (confirmado en Google Maps 3D).
  Tres anclas la dejan a nivel de playa. En total, la pista sube 35 m en lugar de 111 m y su altura va de 3 a 11 m.
- **OSM de Lima casi no etiqueta anchos:** el 100 % del ancho de esta pista es estimado.

### Criterios de salida de S2

- [x] Un tramo real conducible de punta a punta sin saltos ni huecos (prueba automática).
- [x] Puentes y túneles con altura interpolada; estado y diagnósticos en la pista.
- [ ] Manejarlo a mano con el perfil calibrado y confirmar que se siente bien.
- [x] Revisar las lomas del perfil y, si la vía es plana donde el modelo dice que no, fijar anclas (km 2).

Cuando cambies el compilador, regenera la pista con el comando de arriba: la prueba
`committed_asset_is_up_to_date` la reconstruye con los puntos, opciones y anclas que trae en `source` y falla si
`assets/tracks/costa-verde-sprint.json` quedó desactualizada.

Para depurar un tramo: `--example near` lista las calles cerca de un punto y marca las del recorrido, y
`--example edgeprof` muestra la dirección y la altura del modelo a lo largo de unas calles.

## S3 · Intersecciones

Circuito de prueba de 650 m en San Borja (sector Las Artes Norte), alrededor de dos manzanas, con 7 cruces:
gira y sigue derecho en dos cruces de cuatro brazos y pasa por cinco T de todas las formas (entra a la calle
que termina, sale de ella y pasa de largo). Los dos cruces de referencia son Monet × Rousseau (T) y
Milet × Rousseau (cuatro brazos).

```bash
node tools/data-fetch/fetch-osm.mjs data/queries/lima-san-borja.overpassql lima-san-borja
cargo run -p compiler-cli --release -- ingest data/raw/lima-san-borja-2026-11-04.overpass.json \
  --region lima-san-borja --out data/graphs/lima-san-borja.json
cargo run -p compiler-cli --release -- track data/graphs/lima-san-borja.json \
  --dem data/raw/Copernicus_DSM_COG_10_S13_00_W077_00_DEM.tif \
  --from -12.091213,-76.991260 --via -12.090062,-76.991880 --via -12.090982,-76.992709 \
  --via -12.091398,-76.991666 --to -12.091213,-76.991260 --closed --network \
  --id san-borja-las-artes --name "San Borja · Las Artes Norte" --out assets/tracks/san-borja-las-artes.json
```

En el laboratorio: **Sesión → Pista activa** o `?pista=san-borja-las-artes`. Las calles laterales se ven
pero están cerradas con barreras de carrera, como en los NFS clásicos: franjas blancas y negras con flechas
naranjas que apuntan hacia donde sigue la carrera. **R** devuelve el auto al último punto donde estuvo sobre
el circuito (no al más cercano, que podría estar más adelante: eso sería un atajo).

### Cómo se arma la red (`--network`)

- **Corredores:** cada calle entre dos cruces del circuito, con calzada de 6,5 m y vereda de 2,5 m a cada
  lado; detrás, el muro (la línea de fachadas). Las calles laterales que no llevan a otro cruce del circuito
  se cierran con un muro a 30 m.
- **Cruces:** cada calle llega con su dirección y su ancho. Entre dos calles vecinas, sus bordes se cortan en
  una esquina: la de la calzada se redondea con 5 m de radio y la de la fachada queda en ángulo. Cada calle se
  corta en su *boca*, un segmento perpendicular que es exactamente la primera sección del corredor: cruce y
  calle comparten vértices, sin huecos ni escalones. Dos calles a 165° o más se tratan como una vía que sigue.
- **Estados:** `VALID` (T y cruces con ángulos de 60° o más y anchos parecidos), `DEGRADED` (ángulos
  cerrados, anchos muy distintos, todas las calles hacia un lado) y `UNSUPPORTED` (cinco brazos o más, calles
  a distinto nivel, ángulos de menos de 30° o dos cruces tan cerca que sus áreas se superponen).
- **Altura:** un campo 2D en lugar del perfil a lo largo del eje, para que dos calles den la misma altura en
  el mismo punto: percentil 20 del modelo en 24 m a la redonda (quita edificios) y suavizado de 16 m.
- **Barreras de carrera:** una en la boca de cada calle lateral, de fachada a fachada. Son sólidas como un
  muro; las flechas siguen el sentido en que el circuito sale de ese cruce. Para el modo Exploración bastará
  con no cargarlas.
- **Línea de referencia** (cronómetro y piloto automático): el eje de cada calle y, donde el circuito dobla,
  un arco paralelo a la esquina (radio de esquina + medio ancho). `sim-core` arma la superficie y los muros
  directamente desde la red (`network` en la pista); la cinta ya no se usa para eso.

### Lo que aprendimos

- **Los 7 cruces salen `VALID` y el piloto automático da vueltas de 28,2 s** sin tocar muros, sin ruedas en el
  aire y con un golpe vertical máximo de 0,02 m/s (`sim-core/tests/junctions.rs`). Las pruebas del compilador
  comprueban que la superficie no tiene huecos (todo borde suelto está cubierto por un muro), que la línea de
  referencia va siempre sobre la calzada y que la pista de `assets/` está al día (`compiler-cli/tests/s3.rs`).
- **Lo difícil no fue el polígono del cruce**, que salió casi a la primera, sino tres cosas alrededor:
  - *Costuras:* las veredas de las esquinas agregaban vértices sobre el cordón que la calzada no tenía. No se
    ve, pero la malla queda abierta; se resolvió triangulando en cremallera, sin vértices nuevos.
  - *Doblar:* el eje suavizado corta las esquinas. Hizo falta una línea de referencia con arcos reales.
  - *El piloto automático:* medía la curvatura en ±12 m y entraba demasiado rápido a una esquina de 8 m de
    radio; y al mirar lejos cortaba por dentro hasta chocar con la esquina del edificio. Ahora usa también la
    curvatura en ±4 m y acorta la mirada en curvas cerradas. Además frena como un piloto: calcula cuánto
    necesita para llegar a la velocidad de cada curva y frena tarde y fuerte, en vez de ir soltando desde lejos
    (las vueltas bajaron de 36,8 a 28,2 s).
- **La altura 1D de S2 no sirve en una red:** cada calle tendría su propio perfil y no coincidirían en el
  cruce. El campo 2D resuelve San Borja, pero no sirve en un acantilado ni para dos niveles.

### Esfuerzo medido

| Pieza | Líneas | Notas |
|---|---|---|
| Cruces T y de cuatro brazos (`network.rs`) | ~800 | corredores, bocas, esquinas, veredas, estados, línea de referencia |
| Altura como campo 2D (`terrain.rs`) | ~130 | |
| `sim-core`: superficie y muros desde la red | ~150 | más el ajuste del piloto automático |
| Pruebas | ~250 | huecos, línea de referencia, vueltas automáticas |

Una sesión de trabajo de punta a punta, contando la verificación de datos y el trazado del circuito. Lo que
deja ver para lo que falta:

- **Rotondas:** son varios cruces muy juntos sobre un anillo. Hoy se marcan `UNSUPPORTED` por superposición.
  Hace falta agrupar cruces cercanos en uno solo y armar el anillo como corredor: del orden de lo hecho para
  T y cruces, porque reutiliza bocas, esquinas y cremallera.
- **Intercambios y pasos a desnivel:** son lo más caro. La red asume un solo nivel (la altura es una función
  de la posición), así que un puente sobre otra calle no se puede representar todavía. Hay que separar la red
  por niveles (`layer`, `bridge`) y dar a cada nivel su propia altura, con rampas entre ellos. Es más que todo
  S3 junto.
- **Calles de doble calzada y anchos reales:** OSM de Lima casi no trae anchos; con 6,5 m para todo, una
  avenida queda angosta. Habrá que estimar por tipo de vía o corregir a mano.

### Criterios de salida de S3

- [x] Superficie continua al girar en una T y en un cruce de cuatro brazos (prueba automática).
- [x] Estados `VALID` / `DEGRADED` / `UNSUPPORTED` por cruce, con diagnósticos ubicados en el mapa.
- [x] Esfuerzo medido para estimar rotondas e intercambios.
- [ ] Manejarlo a mano: doblar en los cruces, meterse a las calles laterales y volver.
- [ ] Puentes separados por nivel: queda para cuando haya un caso real en el circuito (ver intercambios).

## S4 · Determinismo

Ocho replays dorados se graban en nativo (`crates/sim-core/tests/golden.rs`) y se re-simulan con el binario
WASM en cada entorno. Cada replay guarda un hash del estado cada 5 s: el resultado tiene que coincidir en todos,
no solo el final.

| Pista | Casos |
|---|---|
| `test-loop` | piloto automático, guion de teclado, adversos |
| `costa-verde-sprint` | piloto automático (90 s), adversos |
| `san-borja-las-artes` | piloto automático, guion de teclado, adversos |

Los adversos son una secuencia pseudoaleatoria fija: a fondo contra muros y barreras, volante de tope a tope,
freno de mano, resets y reinicios.

```bash
pnpm install
pnpm turbo run build --filter @rumbo/sim-wasm    # compila el WASM
cd ci/determinism
node run.mjs --browser node          # o chromium, firefox, webkit (Playwright), safari (safaridriver)
```

En GitHub Actions (`.github/workflows/determinism.yml`), cada push corre las pruebas nativas, compila el WASM,
lo verifica en Node y en Chromium y Firefox (Linux), WebKit y Safari real (macOS), y publica la tabla en el
resumen del run.

### Resultado

**Cero diferencias:** los 8 replays dan los mismos hashes que en nativo en Node 22, Chrome 156, Firefox 157,
WebKit y Safari 26.6. Cada replay tarda entre 0,1 y 0,9 s en re-simularse en el navegador.

### Criterios de salida de S4

- [x] Replays de referencia desde S0, ampliados a las tres pistas y a casos adversos.
- [x] Arnés en CI con casos adversos.
- [x] Cero diferencias de hash en Chrome, Firefox y Safari.

## Estado del PoC

| Spike | Pregunta | Respuesta |
|---|---|---|
| S0 · Manejo | ¿Es divertido? | Sí: arcade estilo NFS, 120 Hz, perfil calibrable en vivo. |
| S1 · Datos | ¿El grafo de Lima es ruteable y limpio? | Sí, con ruteo que evita medias vueltas. |
| S2 · Carreteras | ¿Se maneja continuo sobre datos reales? | Sí. La elevación (modelo de superficie) es el punto débil y necesita anclas o un modelo de terreno. |
| S3 · Intersecciones | ¿Cuánto cuesta una intersección? | T y cruces: una sesión. Rotondas: algo parecido. Pasos a desnivel: más que todo S3 junto. |
| S4 · Determinismo | ¿Mismo replay, mismo hash en tres navegadores? | Sí, en cinco entornos. |
| S5 · Compartir | — | Pasa al desarrollo formal. |
| S6 · Visual | — | Pasa al desarrollo formal. |

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

- Medir 60 Hz con subpasos contra 120 Hz y 240 Hz, como pide el documento de arquitectura.
- Calibrar el perfil con teclado y con mando hasta que la respuesta a la pregunta de S0 sea clara.
