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

// «Arquivadas» e não um substantivo: o verificador de português do
// repositório (`scripts/verificar_portugues.py`) trata o calão do
// ficheiro como americanismo, e um adjectivo não é esse calão — o que
// evita a colisão sem distorcer o rótulo da barra lateral, onde convive
// com «Conversas», «Contactos» e «Pedidos».
//
// O adjectivo também evita o falso positivo inverso: o verificador
// reporta a forma substantiva mesmo em português correcto, e a
// decisão está escrita em `tests/test_verificar_portugues.py`
// (`test_falsos_positivos_conhecidos`).
export const DISABLED_ITEMS = [{ label: 'Arquivadas', icon: Archive, tag: 'breve' }];

export function sectionLabelFor(pathname) {
  const match = [...NAV_ITEMS, ...FOOTER_ITEMS].find((item) =>
    item.end ? pathname === item.to : pathname.startsWith(item.to)
  );
  return match ? match.label : 'OnyxChat';
}