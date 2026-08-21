export function median(values) {
  if (values.length === 0) return null;
  const midpoint = Math.floor(values.length / 2);
  return values.length % 2 === 0 ? (values[midpoint - 1] + values[midpoint]) / 2 : values[midpoint];
}

export function percentile(sorted, fraction) {
  if (sorted.length === 0) return null;
  const position = (sorted.length - 1) * fraction;
  const lower = Math.floor(position);
  const upper = Math.ceil(position);
  if (lower === upper) return sorted[lower];
  const weight = position - lower;
  return sorted[lower] * (1 - weight) + sorted[upper] * weight;
}

export function summary(values) {
  const usable = values.filter(Number.isFinite);
  const sorted = [...usable].sort((left, right) => left - right);
  if (sorted.length === 0) {
    return {
      count: 0,
      median: null,
      min: null,
      max: null,
      range: null,
      p25: null,
      p75: null,
      p95: null,
      variance: null,
      standardDeviation: null,
      medianAbsoluteDeviation: null,
      values: [],
    };
  }

  const center = median(sorted);
  const mean = sorted.reduce((total, value) => total + value, 0) / sorted.length;
  const variance = sorted.reduce((total, value) => total + (value - mean) ** 2, 0) / sorted.length;
  const deviations = sorted.map((value) => Math.abs(value - center)).sort((a, b) => a - b);
  return {
    count: sorted.length,
    median: center,
    min: sorted[0],
    max: sorted.at(-1),
    range: sorted.at(-1) - sorted[0],
    p25: percentile(sorted, 0.25),
    p75: percentile(sorted, 0.75),
    p95: percentile(sorted, 0.95),
    variance,
    standardDeviation: Math.sqrt(variance),
    medianAbsoluteDeviation: median(deviations),
    values: usable,
  };
}
