import { Component, createMemo, createSignal, For, Show } from 'solid-js';
import { Sun, Moon } from 'lucide-solid';
import { theme, toggleTheme } from '../lib/theme';

interface SidebarProps {
  currentRoute: string;
}

interface NavItem {
  label: string;
  code: string;
  route: string;
  match: (route: string) => boolean;
}

const navItems: NavItem[] = [
  {
    label: 'Status',
    code: 'STS',
    route: '#/status',
    match: (r) => r === '' || r === '/' || r.startsWith('/status'),
  },
  {
    label: 'Traces',
    code: 'TRC',
    route: '#/traces',
    match: (r) => r.startsWith('/traces'),
  },
  {
    label: 'Logs',
    code: 'LOG',
    route: '#/logs',
    match: (r) => r.startsWith('/logs'),
  },
  {
    label: 'Metrics',
    code: 'MTR',
    route: '#/metrics',
    match: (r) => r.startsWith('/metrics'),
  },
  {
    label: 'Config',
    code: 'CFG',
    route: '#/config',
    match: (r) => r.startsWith('/config'),
  },
];

const Sidebar: Component<SidebarProps> = (props) => {
  const [mobileMenuOpen, setMobileMenuOpen] = createSignal(false);

  const activeIndex = createMemo(() => {
    const route = props.currentRoute;
    const idx = navItems.findIndex((item) => item.match(route));
    return idx >= 0 ? idx : 0;
  });

  const today = new Date().toISOString().slice(0, 10);

  return (
    <>
      {/* Desktop/Tablet sidebar */}
      <aside
        data-testid="sidebar"
        class="w-60 bg-surface-0 border-r-2 border-border flex flex-col h-full shrink-0 max-[960px]:hidden"
      >
        {/* Header — stamped logo */}
        <div class="px-5 py-6 border-b-2 border-border flex justify-center">
          <div class="flex flex-col items-center">
            <span
              data-testid="sidebar-logo"
              class="font-display text-[38px] leading-none tracking-[0.18em] text-accent border-solid border-3 border-accent px-4 pt-2 pb-1.5 inline-block opacity-90"
              style={{
                transform: 'rotate(-2.5deg)',
                "text-shadow": '1px 1px 0 rgba(0,0,0,0.4)',
                outline: '1px solid rgba(255,214,0,0.25)',
                "outline-offset": '3px',
              }}
              aria-label="DevRig"
            >
              DEV RIG
            </span>
            <div class="barcode mt-2.5" aria-hidden="true">
              <span /><span /><span /><span />
              <span /><span /><span /><span />
              <span /><span /><span /><span />
              <span /><span /><span /><span />
              <span /><span /><span /><span />
              <span /><span /><span /><span />
            </div>
            <span class="font-label text-[9px] text-text-muted uppercase tracking-[0.25em] mt-2 text-center whitespace-nowrap">
              Observability Platform
            </span>
          </div>
        </div>

        {/* Navigation */}
        <nav class="flex-1 px-3.5 py-4 flex flex-col gap-0.5">
          <For each={navItems}>
            {(item, index) => {
              const isActive = createMemo(() => activeIndex() === index());
              return (
                <a
                  data-testid="sidebar-nav-item"
                  href={item.route}
                  aria-current={isActive() ? 'page' : undefined}
                  data-active={isActive() ? 'true' : undefined}
                  class={`relative flex items-center gap-2.5 px-3.5 py-3 font-display text-xl tracking-[0.12em] transition-colors ${
                    isActive()
                      ? 'text-accent'
                      : 'text-text-muted hover:text-text-secondary'
                  }`}
                >
                  {/* Active indicator bar */}
                  <Show when={isActive()}>
                    <span class="absolute left-0 top-1 bottom-1 w-1 bg-accent border-solid" />
                  </Show>
                  <span class="uppercase">{item.label}</span>
                  <span
                    class={`font-label text-[9px] tracking-[0.08em] ml-auto ${
                      isActive() ? 'text-text-muted' : 'text-accent/15 group-hover:text-text-muted'
                    }`}
                    aria-hidden="true"
                  >
                    {item.code}
                  </span>
                </a>
              );
            }}
          </For>
        </nav>

        {/* Footer */}
        <div class="px-5 py-3.5 border-t-2 border-border flex items-center justify-between">
          <span class="font-label text-[9px] text-text-muted tracking-[0.08em]">LOT: {today}</span>
          <button
            data-testid="theme-toggle"
            onClick={toggleTheme}
            class="w-6 h-6 flex items-center justify-center text-accent/20 border border-accent/10 hover:text-accent hover:border-border-hover transition-colors"
            aria-label={theme() === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
          >
            {theme() === 'dark' ? <Sun size={12} /> : <Moon size={12} />}
          </button>
        </div>
      </aside>

      {/* Mobile top bar (≤960px) */}
      <div class="hidden max-[960px]:flex w-full bg-surface-0 border-b-2 border-border items-center shrink-0 overflow-hidden">
        {/* Compact logo */}
        <div class="px-4 py-2 border-r-2 border-border shrink-0">
          <span
            class="font-display text-lg leading-none tracking-[0.18em] text-accent border-solid border-2 border-accent px-2.5 pt-1 pb-0.5 inline-block"
            style={{ transform: 'rotate(-2.5deg)' }}
            aria-label="DevRig"
          >
            DEV RIG
          </span>
        </div>

        {/* Horizontal nav — hidden at ≤480px */}
        <nav class="flex-1 flex items-center px-3 gap-0 overflow-x-auto max-[480px]:hidden">
          <For each={navItems}>
            {(item, index) => {
              const isActive = createMemo(() => activeIndex() === index());
              return (
                <a
                  href={item.route}
                  aria-current={isActive() ? 'page' : undefined}
                  class={`relative px-3.5 py-2 font-display text-base tracking-[0.12em] whitespace-nowrap transition-colors ${
                    isActive() ? 'text-accent' : 'text-text-muted hover:text-text-secondary'
                  }`}
                >
                  <Show when={isActive()}>
                    <span class="absolute left-2 right-2 bottom-0 h-0.5 bg-accent border-solid" />
                  </Show>
                  <span class="uppercase">{item.label}</span>
                </a>
              );
            }}
          </For>
        </nav>

        {/* Hamburger — visible at ≤480px */}
        <button
          class="hidden max-[480px]:flex flex-col justify-center gap-1 w-9 h-9 p-2 ml-auto mr-3 border-2 border-border items-center shrink-0"
          aria-label={mobileMenuOpen() ? 'Close menu' : 'Open menu'}
          aria-expanded={mobileMenuOpen()}
          onClick={() => setMobileMenuOpen(!mobileMenuOpen())}
        >
          <span class={`block w-4 h-0.5 bg-accent transition-transform border-solid ${mobileMenuOpen() ? 'translate-y-1.5 rotate-45' : ''}`} />
          <span class={`block w-4 h-0.5 bg-accent transition-opacity border-solid ${mobileMenuOpen() ? 'opacity-0' : ''}`} />
          <span class={`block w-4 h-0.5 bg-accent transition-transform border-solid ${mobileMenuOpen() ? '-translate-y-1.5 -rotate-45' : ''}`} />
        </button>

        {/* Theme toggle in mobile bar */}
        <button
          onClick={toggleTheme}
          class="p-2 mr-3 text-accent/20 border border-accent/10 hover:text-accent shrink-0 max-[480px]:hidden"
          aria-label={theme() === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
        >
          {theme() === 'dark' ? <Sun size={12} /> : <Moon size={12} />}
        </button>
      </div>

      {/* Mobile menu overlay (≤480px) */}
      <Show when={mobileMenuOpen()}>
        <div class="fixed inset-0 top-14 bg-surface-0 z-50 p-6 border-t-2 border-border">
          <nav class="flex flex-col">
            <For each={navItems}>
              {(item, index) => {
                const isActive = createMemo(() => activeIndex() === index());
                return (
                  <a
                    href={item.route}
                    aria-current={isActive() ? 'page' : undefined}
                    class={`font-display text-[28px] tracking-[0.12em] py-4 border-b border-border transition-colors ${
                      isActive() ? 'text-accent' : 'text-text-muted hover:text-text-secondary'
                    }`}
                    onClick={() => setMobileMenuOpen(false)}
                  >
                    <span class="uppercase">{item.label}</span>
                  </a>
                );
              }}
            </For>
          </nav>
        </div>
      </Show>
    </>
  );
};

export default Sidebar;
