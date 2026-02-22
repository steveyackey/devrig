import { Component, createMemo } from 'solid-js';
import { Activity, ScrollText, BarChart3, CircleDot, Settings, Sun, Moon } from 'lucide-solid';
import { theme, toggleTheme } from '../lib/theme';

interface SidebarProps {
  currentRoute: string;
}

interface NavItem {
  label: string;
  icon: Component<{ size?: number; class?: string }>;
  route: string;
  match: (route: string) => boolean;
}

const navItems: NavItem[] = [
  {
    label: 'Status',
    icon: CircleDot,
    route: '#/status',
    match: (r) => r === '' || r === '/' || r.startsWith('/status'),
  },
  {
    label: 'Traces',
    icon: Activity,
    route: '#/traces',
    match: (r) => r.startsWith('/traces'),
  },
  {
    label: 'Logs',
    icon: ScrollText,
    route: '#/logs',
    match: (r) => r.startsWith('/logs'),
  },
  {
    label: 'Metrics',
    icon: BarChart3,
    route: '#/metrics',
    match: (r) => r.startsWith('/metrics'),
  },
  {
    label: 'Config',
    icon: Settings,
    route: '#/config',
    match: (r) => r.startsWith('/config'),
  },
];

const Sidebar: Component<SidebarProps> = (props) => {
  const activeIndex = createMemo(() => {
    const route = props.currentRoute;
    const idx = navItems.findIndex((item) => item.match(route));
    return idx >= 0 ? idx : 0;
  });

  return (
    <aside data-testid="sidebar" class="w-60 bg-surface-1 border-r border-border flex flex-col h-full shrink-0">
      {/* Header */}
      <div class="px-5 py-7 border-b border-border">
        <div class="flex items-center gap-2">
          <div data-testid="sidebar-logo" class="w-8 h-8 rounded-lg bg-accent flex items-center justify-center text-white font-bold text-sm">
            DR
          </div>
          <div>
            <h1 class="text-sm font-semibold text-text-primary">DevRig</h1>
            <p class="text-xs text-text-secondary">Observability</p>
          </div>
        </div>
      </div>

      {/* Navigation */}
      <nav class="flex-1 px-3 py-5 space-y-3">
        {navItems.map((item, index) => {
          const isActive = createMemo(() => activeIndex() === index);
          const Icon = item.icon;
          return (
            <a
              data-testid="sidebar-nav-item"
              href={item.route}
              aria-current={isActive() ? 'page' : undefined}
              data-active={isActive() ? 'true' : undefined}
              class={`flex items-center gap-3 px-4 py-3.5 rounded-lg text-sm font-medium transition-colors ${
                isActive()
                  ? 'bg-accent/15 text-accent border border-accent/20'
                  : 'text-text-secondary hover:text-text-primary hover:bg-surface-2 border border-transparent'
              }`}
            >
              <Icon size={18} class={isActive() ? 'text-accent' : 'text-text-secondary'} />
              <span>{item.label}</span>
            </a>
          );
        })}
      </nav>

      {/* Footer */}
      <div class="px-5 py-5 border-t border-border flex items-center justify-between">
        <p class="text-xs text-text-secondary">v0.1.0</p>
        <button
          data-testid="theme-toggle"
          onClick={toggleTheme}
          class="p-2 rounded-md text-text-muted hover:text-text-primary hover:bg-surface-2 transition-colors"
          aria-label={theme() === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
        >
          {theme() === 'dark' ? <Sun size={16} /> : <Moon size={16} />}
        </button>
      </div>
    </aside>
  );
};

export default Sidebar;
