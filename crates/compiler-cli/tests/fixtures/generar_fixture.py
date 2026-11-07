"""Genera mini-grid.overpass.json: un extracto sintético en formato Overpass JSON.

Coordenadas locales en metros (este, norte) alrededor de -12.12, -77.03 (Miraflores),
convertidas a grados con una aproximación plana suficiente para un fixture.

Casos que cubre:
- Cuadrícula de 3x3 cruces cada 100 m (calles H0-H2 de oeste a este, V0-V2 de sur a norte).
- V1 es de un solo sentido hacia el norte.
- Una peatonal en diagonal que comparte nodos con la cuadrícula (no debe usarse).
- Un puente norte-sur en e=50 que pasa por los mismos puntos que H0-H2 pero con nodos
  propios: no debe conectarse con ellas.
- Una vía que referencia un nodo inexistente (aviso y corte).
- Una calle privada (queda fuera).
"""
import json

LAT0, LON0 = -12.12, -77.03
M_PER_DEG_LAT = 110_580.0
M_PER_DEG_LON = 108_840.0

nodes, ways = {}, []

def node(nid, e, n):
    nodes[nid] = {"type": "node", "id": nid, "lat": round(LAT0 + n / M_PER_DEG_LAT, 7), "lon": round(LON0 + e / M_PER_DEG_LON, 7)}
    return nid

def way(wid, ids, **tags):
    ways.append({"type": "way", "id": wid, "nodes": ids, "tags": tags})

grid = lambda i, j: 1000 + 10 * i + j  # cruce en e=100i, n=100j
for i in range(3):
    for j in range(3):
        node(grid(i, j), 100 * i, 100 * j)

# Calles este-oeste con nodos intermedios (no son cruces) en e=50 y e=150.
for j in range(3):
    mid_a = node(2000 + 10 * j, 50, 100 * j)
    mid_b = node(2001 + 10 * j, 150, 100 * j)
    tags = {"highway": "residential", "name": f"Calle H{j}"}
    if j == 2:
        tags.update({"width": "7.5 m", "lanes": "2"})
    way(10 + j, [grid(0, j), mid_a, grid(1, j), mid_b, grid(2, j)], **tags)

# Calles norte-sur; V1 de un solo sentido hacia el norte.
for i in range(3):
    tags = {"highway": "residential", "name": f"Calle V{i}"}
    if i == 1:
        tags["oneway"] = "yes"
    way(20 + i, [grid(i, 0), grid(i, 1), grid(i, 2)], **tags)

# Peatonal en diagonal (comparte nodos, pero no es conducible).
way(30, [grid(0, 0), grid(1, 1), grid(2, 2)], highway="footway")

# Puente: pasa por (50, 0), (50, 100) y (50, 200) con nodos propios.
bridge = [node(3000 + k, 50, n) for k, n in enumerate([-60, 0, 100, 200, 260])]
way(40, bridge, highway="primary", bridge="yes", layer="1", name="Puente de prueba", maxspeed="60")

# Vía con un nodo faltante entre dos nodos reales.
node(4000, 300, 200)
way(50, [grid(2, 2), 9999, 4000], highway="residential", name="Calle cortada")

# Calle privada.
node(5000, -80, -80)
way(60, [grid(0, 0), 5000], highway="residential", access="private")

doc = {
    "version": 0.6,
    "generator": "generar_fixture.py",
    "osm3s": {"timestamp_osm_base": "2026-11-03T00:00:00Z", "copyright": "Datos sintéticos de prueba"},
    "elements": list(nodes.values()) + ways,
}
with open("mini-grid.overpass.json", "w") as f:
    json.dump(doc, f, indent=1)
print(f"{len(nodes)} nodos, {len(ways)} vías")
