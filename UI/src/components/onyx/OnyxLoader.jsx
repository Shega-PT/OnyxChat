import React from 'react';
import { cn } from '@/lib/utils';

const SEGMENTS = 8;

/** Carregador geométrico — setores radiais em rotação lenta, sem spinner genérico. */
export default function OnyxLoader({ size = 40, label = undefined, className = undefined, block = true }) {
  return (
    <div className={cn('flex w-full items-center justify-center', block && 'py-10', className)}>
      <div className="flex flex-col items-center gap-3">
        <svg
          width={size}
          height={size}
          viewBox="0 0 48 48"
          className="animate-onyx-sweep"
          style={{ animationDuration: '1.5s' }}
          aria-hidden="true"
        >
          <circle cx="24" cy="24" r="21" fill="none" stroke="hsl(var(--elevated-1))" strokeWidth="1" />
          {Array.from({ length: SEGMENTS }).map((_, index) => {
            const outer = 21;
            const inner = 13.5;
            const start = (index / SEGMENTS) * Math.PI * 2;
            const end = start + (Math.PI * 2 / SEGMENTS) * 0.62;
            return (
              <path
                key={index}
                d={[
                  `M${24 + Math.cos(start) * outer} ${24 + Math.sin(start) * outer}`,
                  `A${outer} ${outer} 0 0 1 ${24 + Math.cos(end) * outer} ${24 + Math.sin(end) * outer}`,
                  `L${24 + Math.cos(end) * inner} ${24 + Math.sin(end) * inner}`,
                  `A${inner} ${inner} 0 0 0 ${24 + Math.cos(start) * inner} ${24 + Math.sin(start) * inner}`,
                  'Z',
                ].join(' ')}
                fill="hsl(var(--metallic-1))"
                fillOpacity={0.12 + index * 0.1}
              />
            );
          })}
          <circle cx="24" cy="24" r="2" fill="hsl(var(--metallic-2))" className="animate-onyx-pulse" />
        </svg>
        {label && <span className="onyx-label">{label}</span>}
      </div>
    </div>
  );
}