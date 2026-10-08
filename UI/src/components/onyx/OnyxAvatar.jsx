import React from 'react';
import { cn } from '@/lib/utils';
import { sementeDeAvatar } from '@/lib/onyx/format';

// Ver a nota sobre valores por omissão explícitos em `OnyxBadge.jsx`.

const WEDGES = 8;

const STATUS_COLOR = {
  online: 'bg-onyx-success',
  away: 'bg-onyx-warning',
  busy: 'bg-onyx-error',
  offline: 'bg-onyx-metallic',
};

/**
 * Avatar geométrico determinístico — nunca fotografias.
 *
 * O desenho são oito sectores radiais cuja opacidade individual é derivada
 * do `seed`, o que dá a cada identidade uma marca reconhecível sem
 * recorrer a imagens. O matiz roda num intervalo estreito à volta do
 * ciano da paleta (`184` a `208`) para que nenhum avatar saia do tom do
 * sistema de design.
 *
 * `flat` reduz a saturação e o brilho, para avatares que servem de fundo
 * a outro elemento e não podem competir com o texto.
 */
export default function OnyxAvatar({
  seed = 'onyx',
  name = undefined,
  size = 36,
  status = undefined,
  className = undefined,
  flat = false,
}) {
  const hash = sementeDeAvatar(seed || name || 'onyx');
  const hue = 184 + (hash % 24);
  const saturation = flat ? 10 : 24 + (hash % 12);
  const baseLight = 10 + ((hash >> 4) % 6);

  return (
    <span className={cn('relative inline-flex shrink-0', className)} style={{ width: size, height: size }}>
      <svg viewBox="0 0 48 48" width={size} height={size} className="block">
        <circle cx="24" cy="24" r="23" fill={`hsl(${hue} ${saturation}% ${baseLight}%)`} />
        {Array.from({ length: WEDGES }).map((_, index) => {
          const start = (index / WEDGES) * Math.PI * 2 - Math.PI / 2;
          const end = ((index + 1) / WEDGES) * Math.PI * 2 - Math.PI / 2;
          const x0 = 24 + Math.cos(start) * 23;
          const y0 = 24 + Math.sin(start) * 23;
          const x1 = 24 + Math.cos(end) * 23;
          const y1 = 24 + Math.sin(end) * 23;
          const opacity = 0.22 + (((hash >> (index * 2)) & 3) / 3) * 0.5;
          return (
            <path
              key={index}
              d={`M24 24 L${x0.toFixed(2)} ${y0.toFixed(2)} A23 23 0 0 1 ${x1.toFixed(2)} ${y1.toFixed(2)} Z`}
              fill={`hsl(${hue} ${saturation + 8}% ${baseLight + 26}%)`}
              fillOpacity={opacity}
              stroke="hsl(var(--background))"
              strokeOpacity="0.55"
              strokeWidth="1"
            />
          );
        })}
        <circle cx="24" cy="24" r="13.5" fill="none" stroke={`hsl(${hue} 12% 76%)`} strokeOpacity="0.3" strokeWidth="1" />
        <circle cx="24" cy="24" r="5.5" fill="none" stroke={`hsl(${hue} 12% 82%)`} strokeOpacity="0.42" strokeWidth="1" />
        <circle cx="24" cy="24" r="1.8" fill={`hsl(${hue} 14% 86%)`} fillOpacity="0.85" />
      </svg>
      {status && (
        <span
          className={cn(
            'absolute -bottom-0.5 -right-0.5 rounded-full border-2 border-onyx-surface',
            STATUS_COLOR[status] || STATUS_COLOR.offline
          )}
          style={{ width: Math.max(8, size * 0.28), height: Math.max(8, size * 0.28) }}
        />
      )}
    </span>
  );
}