import React from 'react';
import { NavLink } from 'react-router-dom';
import { cn } from '@/lib/utils';
import OnyxAvatar from '@/components/onyx/OnyxAvatar';
import OnyxStatus from '@/components/onyx/OnyxStatus';
import { useOnyxUI } from '@/lib/onyx/onyx-context';
import { DISABLED_ITEMS, FOOTER_ITEMS, NAV_ITEMS } from '@/lib/onyx/nav';

function Badge({ value = 0 }) {
  if (!value) return null;
  return (
    <span className="ml-auto rounded border border-onyx-line bg-onyx-surface3 px-1.5 font-mono text-[10px] leading-4 text-onyx-text2">
      {value}
    </span>
  );
}

function NavRow({ item, badgeValue, onNavigate = undefined }) {
  const Icon = item.icon;
  return (
    <NavLink
      to={item.to}
      end={item.end}
      onClick={onNavigate}
      className={({ isActive }) =>
        cn(
          'group flex h-9 items-center gap-2.5 rounded-md px-2.5 text-[12.5px] transition-colors duration-150',
          isActive
            ? 'bg-onyx-elevated text-onyx-text'
            : 'text-onyx-text2 hover:bg-onyx-surface3 hover:text-onyx-text'
        )
      }
    >
      {({ isActive }) => (
        <>
          <Icon className={cn('h-4 w-4 shrink-0', isActive ? 'text-onyx-metallic2' : 'text-onyx-text3 group-hover:text-onyx-text2')} />
          <span className="truncate">{item.label}</span>
          <Badge value={badgeValue(item.badge)} />
        </>
      )}
    </NavLink>
  );
}

export default function Sidebar({ className, onNavigate = undefined }) {
  const { identity, counts, openModal } = useOnyxUI();

  const badgeValue = (key) => {
    if (key === 'unread') return counts.unread || null;
    if (key === 'requests') return counts.requests || null;
    return null;
  };

  return (
    <aside className={cn('w-[288px] shrink-0 flex-col border-r border-onyx-line bg-onyx-surface', className)}>
      <div className="shrink-0 p-3">
        <button
          type="button"
          onClick={() => openModal({ type: 'identityCard' })}
          className="w-full rounded-lg border border-onyx-line bg-onyx-surface2 p-3 text-left transition-colors duration-150 hover:border-onyx-metallic/30 hover:bg-onyx-surface3"
        >
          <div className="flex items-center gap-3">
            <OnyxAvatar seed={identity?.id || 'onyx'} name={identity?.name} size={40} status={identity?.status} />
            <div className="min-w-0">
              <p className="truncate text-[12.5px] font-medium text-onyx-text">{identity?.name || '…'}</p>
              <p className="truncate font-mono text-[10px] tracking-wide text-onyx-text3">
                {identity?.identifier || 'a ligar…'}
              </p>
            </div>
          </div>
          <div className="mt-2.5 flex items-center gap-2 border-t border-onyx-line pt-2.5">
            <OnyxStatus status={identity?.status || 'offline'} size="sm" />
            <span className="ml-auto font-mono text-[10px] tracking-wide text-onyx-text3">
              {identity?.role || ''}
            </span>
          </div>
        </button>
      </div>

      <nav className="min-h-0 flex-1 space-y-0.5 overflow-y-auto px-2 pb-2">
        {NAV_ITEMS.map((item) => (
          <NavRow key={item.to} item={item} badgeValue={badgeValue} onNavigate={onNavigate} />
        ))}

        <div className="mt-3 px-2.5 pb-1.5">
          <span className="onyx-label">Sistema</span>
        </div>

        {FOOTER_ITEMS.map((item) => (
          <NavRow key={item.to} item={item} badgeValue={badgeValue} onNavigate={onNavigate} />
        ))}

        {DISABLED_ITEMS.map((item) => (
          <div
            key={item.label}
            aria-disabled="true"
            className="flex h-9 cursor-not-allowed items-center gap-2.5 rounded-md px-2.5 text-[12.5px] text-onyx-text3/60"
          >
            <item.icon className="h-4 w-4 shrink-0" />
            <span className="truncate">{item.label}</span>
            <span className="ml-auto rounded border border-onyx-line px-1.5 font-mono text-[10px] leading-4">
              {item.tag}
            </span>
          </div>
        ))}
      </nav>

      <footer className="shrink-0 border-t border-onyx-line px-3.5 py-2.5">
        <div className="flex items-center justify-between">
          <span className="onyx-label">Nó local</span>
          <span className="font-mono text-[10px] tracking-wide text-onyx-text3">ONYX-SP 4.2</span>
        </div>
        <div className="mt-1.5 flex items-center gap-3">
          <OnyxStatus status="online" size="sm" label={`${counts.peersOnline}/${counts.peersTotal} peers`} />
          <span className="font-mono text-[10px] tracking-wide text-onyx-text3">{counts.relays} relays</span>
        </div>
      </footer>
    </aside>
  );
}