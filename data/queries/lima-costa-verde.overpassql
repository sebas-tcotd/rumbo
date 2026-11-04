// Red vial conducible de Miraflores, Barranco, parte de San Isidro y el circuito de
// playas de la Costa Verde. Solo las clases que acepta el grafo de S1
// (sin service, veredas ni ciclovías). Caja: sur, oeste, norte, este.
[out:json][timeout:120];
(
  way["highway"~"^(motorway|trunk|primary|secondary|tertiary|unclassified|residential|living_street|motorway_link|trunk_link|primary_link|secondary_link|tertiary_link)$"]
    (-12.160,-77.070,-12.095,-77.010);
);
(._;>;);
out body;
