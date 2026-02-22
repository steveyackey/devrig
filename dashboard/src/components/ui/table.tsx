import { splitProps, type JSX, type Component } from 'solid-js';
import { cn } from '../../lib/cn';

const Table: Component<JSX.HTMLAttributes<HTMLTableElement>> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <table class={cn('w-full', local.class)} {...rest}>
      {local.children}
    </table>
  );
};

const TableHeader: Component<JSX.HTMLAttributes<HTMLTableSectionElement>> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <thead class={cn('sticky top-0 z-10', local.class)} {...rest}>
      {local.children}
    </thead>
  );
};

const TableBody: Component<JSX.HTMLAttributes<HTMLTableSectionElement>> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <tbody class={cn(local.class)} {...rest}>
      {local.children}
    </tbody>
  );
};

const TableRow: Component<JSX.HTMLAttributes<HTMLTableRowElement>> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <tr class={cn('border-b border-border hover:bg-accent/[0.03] transition-colors', local.class)} {...rest}>
      {local.children}
    </tr>
  );
};

const TableHead: Component<JSX.ThHTMLAttributes<HTMLTableCellElement>> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <th
      class={cn(
        'bg-surface-1 text-[10px] font-label text-text-muted uppercase tracking-[0.15em] px-5 py-3',
        local.class
      )}
      {...rest}
    >
      {local.children}
    </th>
  );
};

const TableCell: Component<JSX.TdHTMLAttributes<HTMLTableCellElement>> = (props) => {
  const [local, rest] = splitProps(props, ['class', 'children']);
  return (
    <td class={cn('px-5 py-5', local.class)} {...rest}>
      {local.children}
    </td>
  );
};

export { Table, TableHeader, TableBody, TableRow, TableHead, TableCell };
