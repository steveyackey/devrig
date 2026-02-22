import { Component, createMemo } from 'solid-js';

interface SidebarProps {
  currentRoute: string;
}

interface NavItem {
  label: string;
  icon: string;
  route: string;
  match: (route: string) => boolean;
}

const navItems: NavItem[] = [
  {
    label: 'Traces',
    icon: '\u2261',  // hamburger-like lines
    route: '#/traces',
    match: (r) => r === '' || r === '/' || r.startsWith('/traces'),
  },
  {
    label: 'Logs',
    icon: '\u25A4',  // square with horizontal lines
    route: '#/logs',
    match: (r) => r.startsWith('/logs'),
  },
  {
    label: 'Metrics',
    icon: '\u25B3',  // triangle
    route: '#/metrics',
    match: (r) => r.startsWith('/metrics'),
  },
  {
    label: 'Status',
    icon: '\u25C9',  // fisheye
    route: '#/status',
    match: (r) => r.startsWith('/status'),
  },
];

const Sidebar: Component<SidebarProps> = (props) => {
  const activeIndex = createMemo(() => {
    const route = props.currentRoute;
    const idx = navItems.findIndex((item) => item.match(route));
    return idx >= 0 ? idx : 0;
  });

  return (
    <aside class="w-56 bg-zinc-900 border-r border-zinc-700/50 flex flex-col h-full shrink-0">
      {/* Header */}
      <div class="px-5 py-5 border-b border-zinc-700/50">
        <div class="flex items-center gap-2">
          <div class="w-8 h-8 rounded-lg bg-blue-500 flex items-center justify-center text-white font-bold text-sm">
            DR
          </div>
          <div>
            <h1 class="text-sm font-semibold text-zinc-100">DevRig</h1>
            <p class="text-xs text-zinc-500">Observability</p>
          </div>
        </div>
      </div>

      {/* Navigation */}
      <nav class="flex-1 px-3 py-4 space-y-1">
        {navItems.map((item, index) => {
          const isActive = createMemo(() => activeIndex() === index);
          return (
            <a
              href={item.route}
              class={`flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-colors ${
                isActive()
                  ? 'bg-blue-500/15 text-blue-400 border border-blue-500/20'
                  : 'text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800 border border-transparent'
              }`}
            >
              <span class="text-lg w-5 text-center">{item.icon}</span>
              <span>{item.label}</span>
            </a>
          );
        })}
      </nav>

      {/* Footer */}
      <div class="px-5 py-4 border-t border-zinc-700/50">
        <p class="text-xs text-zinc-600">DevRig Dashboard v0.1.0</p>
      </div>
    </aside>
  );
};

export default Sidebar;
