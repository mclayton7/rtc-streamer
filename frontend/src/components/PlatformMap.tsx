import { useEffect, useRef } from 'react';
import L from 'leaflet';
import { buildFovSector } from '../geo';
import type { KlvFieldWithChange } from '../types';

interface Props {
  fields: KlvFieldWithChange[];
}

export function PlatformMap({ fields }: Props) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<L.Map | null>(null);
  const uavMarkerRef = useRef<L.Marker | null>(null);
  const frameCenterRef = useRef<L.Marker | null>(null);
  const fovPolygonRef = useRef<L.Polygon | null>(null);
  const mapCenteredRef = useRef(false);

  // Create map once on mount
  useEffect(() => {
    if (!containerRef.current || mapRef.current) return;

    const map = L.map(containerRef.current, { zoomControl: true }).setView([20, 0], 2);
    L.tileLayer('https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', {
      attribution:
        '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors',
      maxZoom: 19,
    }).addTo(map);
    mapRef.current = map;

    return () => {
      map.remove();
      mapRef.current = null;
      uavMarkerRef.current = null;
      frameCenterRef.current = null;
      fovPolygonRef.current = null;
      mapCenteredRef.current = false;
    };
  }, []);

  // Update markers when fields change
  useEffect(() => {
    const map = mapRef.current;
    if (!map) return;

    // Reset markers when there's no data (e.g. after disconnect)
    if (fields.length === 0) {
      uavMarkerRef.current?.remove();
      uavMarkerRef.current = null;
      frameCenterRef.current?.remove();
      frameCenterRef.current = null;
      fovPolygonRef.current?.remove();
      fovPolygonRef.current = null;
      mapCenteredRef.current = false;
      map.setView([20, 0], 2);
      return;
    }

    const byTag: Record<number, string> = {};
    for (const f of fields) byTag[f.tag] = f.value;

    const sensorLat = parseFloat(byTag[13]);
    const sensorLon = parseFloat(byTag[14]);
    const frameLat = parseFloat(byTag[23]);
    const frameLon = parseFloat(byTag[24]);
    const heading = parseFloat(byTag[5]) || 0;
    const hfov = parseFloat(byTag[16]) || 45;

    if (isNaN(sensorLat) || isNaN(sensorLon)) return;

    const uavIcon = L.divIcon({
      className: 'uav-icon',
      html: `<svg width="28" height="28" viewBox="0 0 28 28"
                  style="transform:rotate(${heading}deg);display:block;filter:drop-shadow(0 1px 3px rgba(0,0,0,0.8))">
               <polygon points="14,3 19,23 14,19 9,23"
                        fill="#a78bfa" stroke="white" stroke-width="1.5" stroke-linejoin="round"/>
             </svg>`,
      iconSize: [28, 28],
      iconAnchor: [14, 14],
    });

    if (uavMarkerRef.current) {
      uavMarkerRef.current.setLatLng([sensorLat, sensorLon]);
      uavMarkerRef.current.setIcon(uavIcon);
    } else {
      uavMarkerRef.current = L.marker([sensorLat, sensorLon], { icon: uavIcon })
        .addTo(map)
        .bindTooltip('Platform', { permanent: false });
    }

    if (!mapCenteredRef.current) {
      map.setView([sensorLat, sensorLon], 14);
      mapCenteredRef.current = true;
    }

    if (!isNaN(frameLat) && !isNaN(frameLon)) {
      const crossIcon = L.divIcon({
        className: '',
        html: `<svg width="16" height="16" viewBox="0 0 16 16"
                     style="display:block;filter:drop-shadow(0 1px 2px rgba(0,0,0,0.8))">
                 <line x1="8" y1="1" x2="8" y2="15" stroke="#34d399" stroke-width="1.5"/>
                 <line x1="1" y1="8" x2="15" y2="8" stroke="#34d399" stroke-width="1.5"/>
                 <circle cx="8" cy="8" r="3" fill="none" stroke="#34d399" stroke-width="1.5"/>
               </svg>`,
        iconSize: [16, 16],
        iconAnchor: [8, 8],
      });

      if (frameCenterRef.current) {
        frameCenterRef.current.setLatLng([frameLat, frameLon]);
      } else {
        frameCenterRef.current = L.marker([frameLat, frameLon], { icon: crossIcon })
          .addTo(map)
          .bindTooltip('Frame Center', { permanent: false });
      }

      const sector = buildFovSector(sensorLat, sensorLon, frameLat, frameLon, hfov);
      if (fovPolygonRef.current) {
        fovPolygonRef.current.setLatLngs(sector);
      } else {
        fovPolygonRef.current = L.polygon(sector, {
          color: '#a78bfa',
          weight: 1.5,
          fillColor: '#a78bfa',
          fillOpacity: 0.12,
          dashArray: '4 3',
        }).addTo(map);
      }
    }
  }, [fields]);

  return (
    <div className="map-section">
      <h3 className="map-title">Platform Map</h3>
      <div ref={containerRef} id="map" />
    </div>
  );
}
