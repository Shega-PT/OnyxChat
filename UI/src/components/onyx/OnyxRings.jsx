import React from 'react';
import { cn } from '@/lib/utils';

const RINGS = [18, 34, 50, 66, 82];
const SPOKES = Array.from({ length: 24 }, (_, index) => index);

/** Estrutura concêntrica decorativa — usada em fundos, estados vazios e cartões. */
export default function OnyxRings({ size = 420, opacity = 0.55, className = undefined }) {
  return (
    <svg
      viewBox="0 0 200 200"
      width={size}
      height={size}
      aria-hidden="true"
      className={cn('pointer-events-none select-none', className)}
      style={{ opacity }}
    >
      {RINGS.map((r) => (
        <circle
          key={r}
          cx="100"
          cy="100"
          r={r}
          fill="none"
          stroke="hsl(var(--metallic-1))"
          strokeOpacity="0.16"
          strokeWidth="0.6"
        />
      ))}
      {SPOKES.map((index) => {
        const angle = (index / SPOKES.length) * Math.PI * 2;
        return (
          <line
            key={index}
            x1={100 + Math.cos(angle) * 18}
            y1={100 + Math.sin(angle) * 18}
            x2={100 + Math.cos(angle) * 86}
            y2={100 + Math.sin(angle) * 86}
            stroke="hsl(var(--metallic-1))"
            strokeOpacity="0.07"
            strokeWidth="0.5"
          />
        );
      })}
    </svg>
  );
}