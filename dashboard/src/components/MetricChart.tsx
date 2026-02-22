import { Component, onMount, onCleanup, createEffect } from 'solid-js';
import uPlot from 'uplot';
import 'uplot/dist/uPlot.min.css';

interface MetricChartProps {
  data: uPlot.AlignedData;
  width: number;
  height: number;
  title?: string;
  seriesLabels?: string[];
  chartType?: 'line' | 'bar';
}

// Color palette for multi-series
const SERIES_COLORS = [
  '#FFD600', // yellow (accent)
  '#4ADE80', // green (success)
  '#60a5fa', // blue (info)
  '#fbbf24', // amber (warning)
  '#f87171', // red (error)
  '#a78bfa', // violet
  '#2dd4bf', // teal
  '#fb923c', // orange
];

function getComputedColor(varName: string): string {
  return getComputedStyle(document.documentElement)
    .getPropertyValue(varName)
    .trim() || '#64748b';
}

const MetricChart: Component<MetricChartProps> = (props) => {
  let containerRef: HTMLDivElement | undefined;
  let chart: uPlot | null = null;

  const buildOpts = (): uPlot.Options => {
    const gridColor = getComputedColor('--color-surface-3') || '#243049';
    const textColor = getComputedColor('--color-text-secondary') || '#94a3b8';
    const borderColor = getComputedColor('--color-border') || '#253045';

    const series: uPlot.Series[] = [{}]; // first series is always x-axis (timestamps)

    const numDataSeries = props.data.length - 1;
    for (let i = 0; i < numDataSeries; i++) {
      const color = SERIES_COLORS[i % SERIES_COLORS.length];
      const label = props.seriesLabels?.[i] ?? `Series ${i + 1}`;

      if (props.chartType === 'bar') {
        series.push({
          label,
          stroke: color,
          fill: color + '40',
          width: 2,
          paths: uPlot.paths.bars!({ size: [0.6, 100] }),
        });
      } else {
        series.push({
          label,
          stroke: color,
          fill: color + '15',
          width: 2,
          points: { show: false },
        });
      }
    }

    return {
      width: props.width,
      height: props.height,
      title: props.title,
      cursor: {
        show: true,
        drag: { x: false, y: false },
      },
      legend: {
        show: numDataSeries > 1,
      },
      axes: [
        {
          stroke: textColor,
          grid: { stroke: gridColor, width: 1 },
          ticks: { stroke: borderColor, width: 1 },
          font: '11px JetBrains Mono, monospace',
        },
        {
          stroke: textColor,
          grid: { stroke: gridColor, width: 1 },
          ticks: { stroke: borderColor, width: 1 },
          font: '11px JetBrains Mono, monospace',
          size: 60,
        },
      ],
      series,
      scales: {
        x: { time: true },
      },
    };
  };

  onMount(() => {
    if (!containerRef) return;
    const opts = buildOpts();
    chart = new uPlot(opts, props.data, containerRef);
  });

  createEffect(() => {
    // React to data/size changes
    const _data = props.data;
    const _w = props.width;
    const _h = props.height;

    if (chart) {
      chart.setSize({ width: _w, height: _h });
      chart.setData(_data);
    }
  });

  onCleanup(() => {
    chart?.destroy();
    chart = null;
  });

  return <div ref={containerRef} class="uplot-container" />;
};

export default MetricChart;

// Sparkline variant — minimal, no axes
export const Sparkline: Component<{
  data: uPlot.AlignedData;
  width: number;
  height: number;
  color?: string;
}> = (props) => {
  let containerRef: HTMLDivElement | undefined;
  let chart: uPlot | null = null;

  onMount(() => {
    if (!containerRef) return;
    const color = props.color || '#FFD600';
    chart = new uPlot(
      {
        width: props.width,
        height: props.height,
        cursor: { show: false },
        legend: { show: false },
        axes: [{ show: false }, { show: false }],
        series: [
          {},
          {
            stroke: color,
            fill: color + '20',
            width: 1.5,
            points: { show: false },
          },
        ],
        scales: { x: { time: true } },
      },
      props.data,
      containerRef,
    );
  });

  createEffect(() => {
    const _data = props.data;
    if (chart) chart.setData(_data);
  });

  onCleanup(() => {
    chart?.destroy();
    chart = null;
  });

  return <div ref={containerRef} />;
};
