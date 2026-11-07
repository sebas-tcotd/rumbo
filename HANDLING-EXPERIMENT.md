# Experimento de conducción NFS

Rama: `codex/nfs-handling-experiment`. Comparación: `main`.

La física experimental 0.5.0 exige freno de mano activo y dirección filtrada
superior a 0.4 para iniciar el derrape. La entrada opcional por freno normal
usa un umbral de 0.55. No hay entrada diferida al soltar el freno de mano.
El contravolante devuelve el objetivo de deslizamiento a cero y libera la
asistencia; sin nueva intención de derrape, esta termina después de 0.5 s
desde su inicio. Esto termina la asistencia, no garantiza eliminar físicamente
todo deslizamiento en ese instante.

El freno trasero pasa de 0.2 a 0.8 de la fuerza configurada, conservando
el suavizado cerca de cero. El pico lateral se multiplica por 1.4 en arcén
y 1.1 en pianos. Son valores experimentales, no datos verificados de NFS.
Se conservan los perfiles existentes como referencia; no se añaden superficies.

La cámara de seguimiento usa FOV 60–66°, más anticipación a alta velocidad
y roll lateral limitado a unos 3.4°. Acompaña directamente el desplazamiento
del auto y suaviza los cambios de offset, evitando separación acumulada por
velocidad. La distancia normal aumenta de 6.4 a 8.6 m hasta 250 km/h;
la cercana, de 4.2 a 5.45 m, con una reacción pequeña a la aceleración.
La interpolación es exponencial y el delta de render se limita a 0.1 s.

## Prueba manual pendiente

1. Conducir en Costa Verde a unos 80–110 km/h. Pulsar freno de mano durante
   0.15–0.25 s con dirección ya aplicada; soltar y contravolantear.
2. Pulsar brevemente en recta, soltar y luego girar: no debe activar la
   asistencia de derrape. Una pulsación simultánea de menos de 0.1 s puede
   no superar la rampa de dirección; es comportamiento deliberado.
3. Frenar en curva sin freno de mano; comprobar también detención y frenado
   con velocidad hacia atrás, sin oscilaciones cerca de cero.
4. Comparar asfalto, pianos y arcén. Probar cámara cercana, seguimiento y aérea,
   reinicio y pausas; evaluar mareo, visibilidad y recuperación de la cámara.
5. Comparar con `main` usando el mismo perfil y maniobras. No comparar hashes
   de replays 0.4.0: los dorados y sus huellas por tick se regeneraron para 0.5.0.

La validación automática cubre manejo, determinismo nativo y replays. La
sensación de control y la comodidad visual requieren la prueba manual.
