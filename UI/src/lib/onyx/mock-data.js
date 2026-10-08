// Dados de demonstração do OnyxChat.
// Substituir pelos dados reais apenas em data-adapter.js — nenhuma vista lê este ficheiro.

const now = Date.now();
const ago = (minutes) => new Date(now - minutes * 60000).toISOString();

export const identity = {
  id: '8F3Q5S',
  name: 'Marta Vasconcelos',
  status: 'online',
  role: 'Nó primário',
  fingerprint: 'A4:7C:19:EE:52:B0:3D:81:C7:6F:24:90:11:8B:5A:D2',
  keyAlgorithm: 'Ed25519 · X25519',
  createdAt: '2024-02-11',
  region: 'Lisboa, PT',
  devices: 3,
  verifiedContacts: 7,
  totalContacts: 11,
  sessionsToday: 4,
};

export const contacts = [
  { id: 'A7K2M4', name: 'Helena Braga', status: 'online', verified: true, role: 'Operadora de nó', note: 'Arquitetura de rede · núcleo', tags: ['Equipa', 'Infraestrutura'], addedAt: '2024-03-02', lastSeen: ago(4), mutual: 6 },
  { id: 'B3X9Q1', name: 'Rui Camacho', status: 'away', verified: true, role: 'Auditor', note: 'Auditoria de chaves', tags: ['Segurança'], addedAt: '2024-05-18', lastSeen: ago(52), mutual: 4 },
  { id: 'C8L4D7', name: 'Inês Faria', status: 'online', verified: false, role: 'Engenheira', note: 'Interoperabilidade', tags: ['Integrações'], addedAt: '2025-01-09', lastSeen: ago(1), mutual: 2 },
  { id: 'D2M6P9', name: 'Tomás Neves', status: 'offline', verified: false, role: 'Contacto externo', note: '', tags: [], addedAt: '2025-04-22', lastSeen: ago(2880), mutual: 0 },
  { id: 'E5R1T3', name: 'Sofia Quintela', status: 'busy', verified: true, role: 'Protocolo', note: 'Sincronização offline', tags: ['Equipa'], addedAt: '2024-09-12', lastSeen: ago(18), mutual: 5 },
  { id: 'F9W2Y6', name: 'Duarte Mealha', status: 'online', verified: true, role: 'Operador', note: 'Infraestrutura de relays', tags: ['Infraestrutura'], addedAt: '2024-07-30', lastSeen: ago(7), mutual: 3 },
  { id: 'G4H8J2', name: 'Clara Antunes', status: 'away', verified: false, role: 'Contacto externo', note: '', tags: [], addedAt: '2025-06-03', lastSeen: ago(130), mutual: 1 },
  { id: 'H7N3Z5', name: 'Nuno Sampaio', status: 'offline', verified: true, role: 'Auditor', note: 'Arquivo de auditoria', tags: ['Segurança', 'Arquivo'], addedAt: '2024-11-27', lastSeen: ago(1500), mutual: 2 },
  { id: 'SRV002', name: 'Nó de arquivo 02', status: 'online', verified: true, role: 'Serviço', note: 'Nó de serviço · replicação', tags: ['Serviço'], addedAt: '2024-08-05', lastSeen: ago(2), mutual: 0, service: true },
];

export const conversations = [
  {
    id: 'c1',
    contactId: 'A7K2M4',
    pinned: true,
    unread: 2,
    muted: false,
    verified: true,
    route: 'direto',
    latency: 19,
    messages: [
      { id: 'c1m1', from: 'them', text: 'Rota direta reestabelecida depois da manutenção. O caminho por relay foi descartado.', at: ago(182), state: 'read' },
      { id: 'c1m2', from: 'me', text: 'Boa. A latência ficou estável?', at: ago(174), state: 'read' },
      { id: 'c1m3', from: 'them', text: 'Sim — 19 ms de mediana em 400 verificações seguidas.', at: ago(170), state: 'read' },
      { id: 'c1m4', from: 'me', text: 'Mantenho a política de rota até à próxima rotação de chaves.', at: ago(96), state: 'read' },
      { id: 'c1m5', from: 'them', text: 'Combinado. Envio o resumo do handshake ainda hoje.', at: ago(38), state: 'read' },
      { id: 'c1m6', from: 'them', text: 'Ficheiro assinado, com as verificações em anexo.', at: ago(12), state: 'read' },
    ],
  },
  {
    id: 'c2',
    contactId: 'B3X9Q1',
    pinned: false,
    unread: 0,
    muted: false,
    verified: true,
    route: 'relay',
    latency: 71,
    messages: [
      { id: 'c2m1', from: 'them', text: 'Confirmas o meu identificador antes de eu partilhar o cartão?', at: ago(322), state: 'read' },
      { id: 'c2m2', from: 'me', text: 'Verifiquei a impressão digital fora do canal. Está igual.', at: ago(300), state: 'read' },
      { id: 'c2m3', from: 'them', text: 'Obrigado. Do meu lado fica marcado como verificado.', at: ago(295), state: 'read' },
    ],
  },
  {
    id: 'c3',
    title: 'Nó de auditoria',
    avatarSeed: 'audit-node',
    members: 6,
    pinned: true,
    unread: 2,
    muted: false,
    verified: true,
    route: 'direto',
    latency: 26,
    messages: [
      { id: 'c3m1', from: 'them', author: 'Sofia Quintela', text: 'Relatório semanal publicado. Três nós com latência acima do limite.', at: ago(224), state: 'read' },
      { id: 'c3m2', from: 'me', text: 'Vou marcar os dois relays alternativos para revisão.', at: ago(212), state: 'read' },
      { id: 'c3m3', from: 'them', author: 'Duarte Mealha', text: 'Feito. Os registos ficam no arquivo do nó até sexta.', at: ago(64), state: 'read' },
      { id: 'c3m4', from: 'them', author: 'Nuno Sampaio', text: 'Anexei a lista de chaves rotacionadas.', at: ago(45), state: 'read' },
    ],
  },
  {
    id: 'c4',
    contactId: 'C8L4D7',
    pinned: false,
    unread: 5,
    muted: false,
    verified: false,
    route: 'direto',
    latency: 23,
    messages: [
      { id: 'c4m1', from: 'them', text: 'Consegues repetir a chave de sessão de ontem?', at: ago(142), state: 'read' },
      { id: 'c4m2', from: 'them', text: 'O cliente novo não reconheceu o handshake.', at: ago(138), state: 'read' },
      { id: 'c4m3', from: 'them', text: 'Fico a aguardar — sem pressa.', at: ago(96), state: 'read' },
      { id: 'c4m4', from: 'them', text: 'Enviei também o registo do lado do servidor.', at: ago(90), state: 'read' },
      { id: 'c4m5', from: 'them', text: 'Obrigada!', at: ago(30), state: 'read' },
    ],
  },
  {
    id: 'c5',
    contactId: 'D2M6P9',
    pinned: false,
    unread: 0,
    muted: true,
    verified: false,
    route: 'relay',
    latency: 92,
    messages: [
      { id: 'c5m1', from: 'them', text: 'Passa-me o teu identificador outra vez, perdi o cartão.', at: ago(2900), state: 'read' },
      { id: 'c5m2', from: 'me', text: 'Envio agora. Confirma a impressão digital antes de adicionar.', at: ago(2880), state: 'delivered' },
      { id: 'c5m3', from: 'them', text: 'Feito, obrigado.', at: ago(2870), state: 'read' },
    ],
  },
  {
    id: 'c6',
    contactId: 'E5R1T3',
    pinned: false,
    unread: 0,
    muted: false,
    verified: true,
    route: 'direto',
    latency: 24,
    messages: [
      { id: 'c6m1', from: 'them', text: 'Testei a recolha de contactos em modo offline. Funciona como esperado.', at: ago(432), state: 'read' },
      { id: 'c6m2', from: 'me', text: 'Boa. Fica com a entrada em cache até à próxima sincronização.', at: ago(421), state: 'read' },
      { id: 'c6m3', from: 'them', text: 'Combinado.', at: ago(416), state: 'read' },
    ],
  },
];

export const requests = [
  { id: 'r1', personId: 'J5K7L9', name: 'Vasco Pontes', status: 'away', verified: false, message: 'Encontrámo-nos na auditoria de março. Podes adicionar-me?', at: ago(48), mutual: 2 },
  { id: 'r2', personId: 'K1P3R5', name: 'Lídia Serra', status: 'online', verified: false, message: 'Preciso de confirmar a tua chave para o arquivo partilhado.', at: ago(266), mutual: 1 },
  { id: 'r3', personId: 'M8N2V6', name: 'Identificador 4A9F21', status: 'online', verified: false, service: true, message: 'Pedido automático de nó de serviço.', at: ago(1520), mutual: 0 },
];

export const network = {
  mode: 'direto',
  modeLabel: 'Ligação direta',
  peersOnline: 7,
  peersTotal: 11,
  relays: 3,
  relaysUsed: 1,
  latencyMs: 21,
  jitterMs: 4,
  transport: 'QUIC · UDP',
  encryption: 'X25519 · AES-256-GCM',
  protocol: 'ONYX-SP 4.2',
  uptime: '18 h 42 min',
  throughput: '128 KB/s',
  sync: { state: 'sincronizado', lastAt: ago(6), pending: 0 },
  nodes: [
    { id: 'A7K2M4', label: 'Helena Braga', region: 'Lisboa', route: 'direto', latency: 19, status: 'online' },
    { id: 'E5R1T3', label: 'Sofia Quintela', region: 'Porto', route: 'direto', latency: 24, status: 'busy' },
    { id: 'SRV002', label: 'Nó de arquivo 02', region: 'FRA-1', route: 'direto', latency: 33, status: 'online' },
    { id: 'F9W2Y6', label: 'Duarte Mealha', region: 'Coimbra', route: 'relay', latency: 58, status: 'online' },
    { id: 'B3X9Q1', label: 'Rui Camacho', region: 'Braga', route: 'relay', latency: 71, status: 'away' },
  ],
  events: [
    { at: ago(12), text: 'Rota direta reestabelecida com A7K2M4', level: 'ok' },
    { at: ago(58), text: 'Relay R-03 assumiu a sessão com B3X9Q1', level: 'info' },
    { at: ago(96), text: 'Sincronização de contactos concluída', level: 'ok' },
    { at: ago(210), text: 'Latência acima de 70 ms para B3X9Q1', level: 'warn' },
  ],
};

export const security = {
  score: 92,
  level: 'Forte',
  checks: [
    { id: 'keys', label: 'Chave de identidade', state: 'ok', detail: 'Ed25519 ativa. Rotação agendada para 24 nov 2026.' },
    { id: 'devices', label: 'Dispositivos autorizados', state: 'ok', detail: '3 de 5 slots em uso. Todos com chave própria.' },
    { id: 'sessions', label: 'Verificação de sessões', state: 'ok', detail: '7 de 8 contactos verificados fora do canal.' },
    { id: 'relay', label: 'Encaminhamento por relay', state: 'ok', detail: 'Sempre cifrado. Nenhum pacote em claro registado.' },
    { id: 'backup', label: 'Cópia de segurança da chave', state: 'warn', detail: 'Última exportação há 41 dias. Recomendado repetir.' },
    { id: 'exposure', label: 'Identificador público', state: 'warn', detail: 'Visível para pedidos de contacto com 2 ligações comuns.' },
  ],
  devices: [
    { id: 'dev1', label: 'Estação principal', os: 'Linux 6.8 · x64', at: ago(5), current: true },
    { id: 'dev2', label: 'Portátil', os: 'macOS 15 · arm64', at: ago(1490) },
    { id: 'dev3', label: 'Telemóvel', os: 'Android 15', at: ago(4310) },
  ],
  alerts: [
    { id: 'a1', level: 'info', text: 'Nova sessão autorizada em Telemóvel', at: ago(4310) },
    { id: 'a2', level: 'warn', text: 'Pedido de contacto bloqueado (identificador desconhecido)', at: ago(600) },
    { id: 'a3', level: 'ok', text: 'Chave de sessão rotacionada com A7K2M4', at: ago(180) },
  ],
  key: {
    algorithm: 'Ed25519 · X25519',
    fingerprint: 'A4:7C:19:EE:52:B0:3D:81:C7:6F:24:90:11:8B:5A:D2',
    rotatedAt: '2026-09-24',
    nextRotation: '2026-12-24',
    backupAt: '2026-08-26',
  },
};

export const settingsSections = [
  {
    id: 'geral',
    label: 'Geral',
    items: [
      { id: 'geral-1', type: 'select', label: 'Idioma da interface', hint: 'Aplica-se a toda a aplicação.', value: 'Português (PT)', options: ['Português (PT)', 'Português (BR)', 'English'] },
      { id: 'geral-2', type: 'select', label: 'Ao iniciar', value: 'Última conversa', options: ['Última conversa', 'Lista de conversas', 'Descobrir'] },
      { id: 'geral-3', type: 'switch', label: 'Confirmar antes de enviar', hint: 'Pede confirmação em mensagens com anexos.', value: false },
    ],
  },
  {
    id: 'aparencia',
    label: 'Aparência',
    items: [
      { id: 'ap-1', type: 'select', label: 'Densidade da interface', value: 'Compacta', options: ['Compacta', 'Confortável'] },
      { id: 'ap-2', type: 'switch', label: 'Pré-visualização da mensagem', hint: 'Mostra o início da última mensagem na lista.', value: true },
      { id: 'ap-3', type: 'switch', label: 'Separadores de dia nas conversas', value: true },
      { id: 'ap-4', type: 'switch', label: 'Contornos de foco reforçados', hint: 'Útil para navegação por teclado.', value: false },
    ],
  },
  {
    id: 'privacidade',
    label: 'Privacidade',
    items: [
      { id: 'pv-1', type: 'select', label: 'Quem pode enviar pedidos', value: 'Com 2 contactos em comum', options: ['Ninguém', 'Com 1 contacto em comum', 'Com 2 contactos em comum', 'Todos'] },
      { id: 'pv-2', type: 'switch', label: 'Aceitar automaticamente ligações diretas', hint: 'Dispensa confirmação quando a rota é verificada.', value: false },
      { id: 'pv-3', type: 'switch', label: 'Ocultar estado de leitura', value: false },
      { id: 'pv-4', type: 'switch', label: 'Ocultar indicador de escrita', value: false },
      { id: 'pv-5', type: 'switch', label: 'Bloquear capturas de ecrã', hint: 'Experimental neste dispositivo.', value: true, badge: 'beta' },
    ],
  },
  {
    id: 'identidade',
    label: 'Identidade',
    items: [
      { id: 'id-1', type: 'text', label: 'Nome apresentado', value: 'Marta Vasconcelos' },
      { id: 'id-2', type: 'readonly', label: 'Identificador', value: 'ONYX-8F3Q5S-!R4%D2#', mono: true, action: 'copy' },
      { id: 'id-3', type: 'readonly', label: 'Impressão digital', value: 'A4:7C:19:EE:…:5A:D2', mono: true, action: 'copy' },
      { id: 'id-4', type: 'switch', label: 'Permitir partilha do cartão', value: true },
      { id: 'id-5', type: 'action', label: 'Exportar chave de identidade', hint: 'Ficheiro cifrado com frase de segurança.', actionLabel: 'Exportar' },
    ],
  },
  {
    id: 'rede',
    label: 'Rede',
    items: [
      { id: 'rd-1', type: 'switch', label: 'Preferir ligação direta', hint: 'Usa relays apenas quando a rota direta falha.', value: true },
      { id: 'rd-2', type: 'switch', label: 'Permitir relays de terceiros', value: true },
      { id: 'rd-3', type: 'select', label: 'Limite de latência para rota direta', value: '50 ms', options: ['30 ms', '50 ms', '100 ms', 'Sem limite'] },
      { id: 'rd-4', type: 'switch', label: 'Poupança de dados', hint: 'Reduz a frequência de sincronização.', value: false },
    ],
  },
  {
    id: 'seguranca',
    label: 'Segurança',
    items: [
      { id: 'sg-1', type: 'switch', label: 'Bloqueio automático', hint: 'Após 10 minutos de inatividade.', value: true },
      { id: 'sg-2', type: 'select', label: 'Rotação de chaves', value: '90 dias', options: ['30 dias', '90 dias', '180 dias'] },
      { id: 'sg-3', type: 'switch', label: 'Confirmar novas sessões', hint: 'Pede verificação em cada novo dispositivo.', value: true },
      { id: 'sg-4', type: 'action', label: 'Cópia de segurança da chave', hint: 'Última exportação há 41 dias.', actionLabel: 'Exportar' },
    ],
  },
  {
    id: 'notificacoes',
    label: 'Notificações',
    items: [
      { id: 'nt-1', type: 'switch', label: 'Mensagens novas', value: true },
      { id: 'nt-2', type: 'switch', label: 'Pedidos de contacto', value: true },
      { id: 'nt-3', type: 'switch', label: 'Alertas de segurança', hint: 'Recomendado manter ativo.', value: true },
      { id: 'nt-4', type: 'switch', label: 'Som de notificação', value: false },
    ],
  },
  {
    id: 'avancado',
    label: 'Avançado',
    items: [
      { id: 'av-1', type: 'readonly', label: 'Protocolo', value: 'ONYX-SP 4.2', mono: true },
      { id: 'av-2', type: 'readonly', label: 'Transporte', value: 'QUIC · UDP 443', mono: true },
      { id: 'av-3', type: 'readonly', label: 'Registo de diagnóstico', value: '32 MB · últimos 7 dias', mono: true },
      { id: 'av-4', type: 'action', label: 'Limpar dados de diagnóstico', actionLabel: 'Limpar' },
      { id: 'av-5', type: 'action', label: 'Reiniciar nó', hint: 'Interrompe as sessões ativas durante alguns segundos.', actionLabel: 'Reiniciar', danger: true },
    ],
  },
];