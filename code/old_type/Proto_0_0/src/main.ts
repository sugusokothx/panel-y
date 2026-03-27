import Plotly from 'plotly.js-dist-min';
import type { WaveformData } from './types';

function isWaveformData(data: unknown): data is WaveformData {
  if (typeof data !== 'object' || data === null) return false;
  const d = data as Record<string, unknown>;
  return (
    Array.isArray(d.time) &&
    Array.isArray(d.voltage) &&
    Array.isArray(d.current)
  );
}

function showError(message: string): void {
  document.querySelector<HTMLDivElement>('#app')!.innerHTML = `
    <div style="color: red; padding: 1rem;">
      <strong>エラー:</strong> ${message}
    </div>
  `;
}

function renderChart(data: WaveformData): void {
  const app = document.querySelector<HTMLDivElement>('#app')!;
  app.innerHTML = '<div id="chart" style="width:100%;height:80vh;"></div>';

  const voltageTrace: Plotly.Data = {
    x: data.time,
    y: data.voltage,
    type: 'scatter',
    mode: 'lines',
    name: 'Voltage [V]',
    xaxis: 'x',
    yaxis: 'y',
  };

  const currentTrace: Plotly.Data = {
    x: data.time,
    y: data.current,
    type: 'scatter',
    mode: 'lines',
    name: 'Current [A]',
    xaxis: 'x2',
    yaxis: 'y2',
  };

  const layout: Partial<Plotly.Layout> = {
    grid: { rows: 2, columns: 1, pattern: 'independent' },
    xaxis: {
      title: 'Time [s]',
      showspikes: true,
      spikemode: 'across',
      spikedash: 'solid',
    },
    yaxis: { title: 'Voltage [V]' },
    xaxis2: {
      title: 'Time [s]',
      matches: 'x',
      showspikes: true,
      spikemode: 'across',
      spikedash: 'solid',
    },
    yaxis2: { title: 'Current [A]' },
    hovermode: 'x unified',
    spikedistance: -1,
  };

  Plotly.newPlot('chart', [voltageTrace, currentTrace], layout, { responsive: true });
}

async function main(): Promise<void> {
  const response = await fetch('./waveform.json').catch(() => null);
  if (!response || !response.ok) {
    showError('waveform.json の取得に失敗しました。');
    return;
  }

  const data: unknown = await response.json().catch(() => null);
  if (!isWaveformData(data)) {
    showError('waveform.json の形式が不正です。time, voltage, current の配列が必要です。');
    return;
  }

  renderChart(data);
}

main();
