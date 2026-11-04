// S3 · Intersecciones: sector Las Artes Norte de San Borja, entre la Av. Javier Prado Este
// y la Av. San Borja Norte. Cuadrícula residencial con un cruce en T (Monet × Rousseau) y
// uno de cuatro brazos (Milet × Rousseau) a pocas cuadras. Mismas clases que la Costa Verde.
// Caja: sur, oeste, norte, este.
[out:json][timeout:120];
(
  way["highway"~"^(motorway|trunk|primary|secondary|tertiary|unclassified|residential|living_street|motorway_link|trunk_link|primary_link|secondary_link|tertiary_link)$"]
    (-12.102,-77.006,-12.080,-76.976);
);
(._;>;);
out body;
