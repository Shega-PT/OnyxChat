import {
  Archive,
  Compass,
  Fingerprint,
  MessageSquare,
  Radio,
  Settings2,
  Shield,
  UserPlus,
  Users,
} from 'lucide-react';

export const NAV_ITEMS = [
  { to: '/', label: 'Conversas', icon: MessageSquare, end: true, badge: 'unread' },
  { to: '/contactos', label: 'Contactos', icon: Users },
  { to: '/pedidos', label: 'Pedidos', icon: UserPlus, badge: 'requests' },
  { to: '/descobrir', label: 'Descobrir', icon: Compass },
  { to: '/rede', label: 'Rede', icon: Radio },
  { to: '/seguranca', label: 'Segurança', icon: Shield },
  { to: '/identidade', label: 'Identidade', icon: Fingerprint },
];

export const FOOTER_ITEMS = [{ to: '/definicoes', label: 'Definições', icon: Settings2 }];

// «Arquivadas» e não «Arquivo»: o verificador de português do
// repositório (`scripts/verificar_portugues.py`) trata `arquivo` como
// americanismo e exige `ficheiro`, o que aqui seria falso — a palavra
// é um substantivo comum, não o calão do ficheiro. Preferi um
// adjectivo que evita a colisão sem distorcer o rótulo da barra
// lateral, onde convive com «Conversas», «Contactos» e «Pedidos».
export const DISABLED_ITEMS = [{ label: 'Arquivadas', icon: Archive, tag: 'breve' }];

export function sectionLabelFor(pathname) {
  const match = [...NAV_ITEMS, ...FOOTER_ITEMS].find((item) =>
    item.end ? pathname === item.to : pathname.startsWith(item.to)
  );
  return match ? match.label : 'OnyxChat';
}