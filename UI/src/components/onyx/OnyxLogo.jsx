import React from 'react';
import { cn } from '@/lib/utils';

const VERTICES = [
  [16, 5],
  [47, 5],
  [57, 19],
  [57, 43],
  [45, 57],
  [20, 57],
  [11, 45],
  [11, 19],
];

const CENTER = [33, 31];
const ARCS = [
  { r: 10, rot: 0 },
  { r: 6.4, rot: -38 },
  { r: 3.2, rot: -76 },
];

/**
 * Marca do OnyxChat: bolha hexagonal facetada com núcleo circular em espiral.
 * tone="primary" usa a cor da marca; tone="flat" usa o metal para contextos neutros.
 */
export default function OnyxLogo({ size = 28, tone = 'primary', showName = false, className = undefined }) {
  const fill = tone === 'primary' ? 'hsl(var(--primary))' : 'hsl(var(--metallic-1))';

  return (
    <span className={cn('inline-flex items-center gap-2.5', className)}>
      <svg width={size} height={size} viewBox="0 0 66 64" fill="none" aria-hidden="true" className="shrink-0">
        <path d="M16 5 H47 L57 19 V43 L45 57 H20 L11 45 V19 Z" fill={fill} />
        <path d="M11 44 L4 61 L23 57 Z" fill={fill} />
        <path
          d="M33 12 L52 31 L33 50 L14 31 Z"
          fill="none"
          stroke="hsl(var(--foreground))"
          strokeOpacity="0.07"
          strokeWidth="1"
        />
        {VERTICES.map(([x, y]) => (
          <line
            key={`${x}-${y}`}
            x1={CENTER[0]}
            y1={CENTER[1]}
            x2={x}
            y2={y}
            stroke="hsl(var(--foreground))"
            strokeOpacity="0.1"
            strokeWidth="1"
          />
        ))}
        <circle cx={CENTER[0]} cy={CENTER[1]} r="13" fill="hsl(var(--background))" fillOpacity="0.94" />
        <circle
          cx={CENTER[0]}
          cy={CENTER[1]}
          r="13"
          fill="none"
          stroke="hsl(var(--metallic-2))"
          strokeOpacity="0.32"
          strokeWidth="1"
        />
        {ARCS.map(({ r, rot }) => (
          <path
            key={r}
            transform={`rotate(${rot} ${CENTER[0]} ${CENTER[1]})`}
            d={`M33 ${31 - r} A${r} ${r} 0 1 1 ${33 - r * 0.2} ${31 - r * 0.98}`}
            fill="none"
            stroke="hsl(var(--metallic-2))"
            strokeOpacity="0.5"
            strokeWidth="1.1"
            strokeLinecap="round"
          />
        ))}
        <circle cx={CENTER[0]} cy={CENTER[1]} r="1.6" fill="hsl(var(--metallic-2))" />
      </svg>
      {showName && (
        <span className="text-[12.5px] font-medium uppercase tracking-[0.22em] text-onyx-text">OnyxChat</span>
      )}
    </span>
  );
}