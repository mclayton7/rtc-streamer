export function bearing(lat1: number, lon1: number, lat2: number, lon2: number): number {
  const f1 = (lat1 * Math.PI) / 180;
  const f2 = (lat2 * Math.PI) / 180;
  const dl = ((lon2 - lon1) * Math.PI) / 180;
  const y = Math.sin(dl) * Math.cos(f2);
  const x = Math.cos(f1) * Math.sin(f2) - Math.sin(f1) * Math.cos(f2) * Math.cos(dl);
  return ((Math.atan2(y, x) * 180) / Math.PI + 360) % 360;
}

export function haversineKm(lat1: number, lon1: number, lat2: number, lon2: number): number {
  const R = 6371;
  const df = ((lat2 - lat1) * Math.PI) / 180;
  const dl = ((lon2 - lon1) * Math.PI) / 180;
  const a =
    Math.sin(df / 2) ** 2 +
    Math.cos((lat1 * Math.PI) / 180) *
      Math.cos((lat2 * Math.PI) / 180) *
      Math.sin(dl / 2) ** 2;
  return R * 2 * Math.atan2(Math.sqrt(a), Math.sqrt(1 - a));
}

export function destinationPoint(
  lat: number,
  lon: number,
  bearingDeg: number,
  distKm: number,
): [number, number] {
  const R = 6371;
  const d = distKm / R;
  const f1 = (lat * Math.PI) / 180;
  const l1 = (lon * Math.PI) / 180;
  const t = (bearingDeg * Math.PI) / 180;
  const f2 = Math.asin(Math.sin(f1) * Math.cos(d) + Math.cos(f1) * Math.sin(d) * Math.cos(t));
  const l2 =
    l1 +
    Math.atan2(
      Math.sin(t) * Math.sin(d) * Math.cos(f1),
      Math.cos(d) - Math.sin(f1) * Math.sin(f2),
    );
  return [(f2 * 180) / Math.PI, (l2 * 180) / Math.PI];
}

export function buildFovSector(
  sensorLat: number,
  sensorLon: number,
  frameLat: number,
  frameLon: number,
  hfovDeg: number,
): [number, number][] {
  const dist = haversineKm(sensorLat, sensorLon, frameLat, frameLon);
  const centerB = bearing(sensorLat, sensorLon, frameLat, frameLon);
  const half = hfovDeg / 2;
  const steps = 32;
  const pts: [number, number][] = [[sensorLat, sensorLon]];
  for (let i = 0; i <= steps; i++) {
    const b = centerB - half + (hfovDeg * i) / steps;
    pts.push(destinationPoint(sensorLat, sensorLon, b, dist));
  }
  return pts;
}
