// from hushbacks packages/ui/src/components/charts/DistributionChart.tsx
import type { ChartConfig } from "@/ui/chart";

import { BarChart, type BarChartProps } from "./BarChart";
import { PieChart, type PieChartProps } from "./PieChart";

export interface DistributionChartProps {
  data: Record<string, unknown>[];
  dataKey: string;
  labelKey?: string;
  chartConfig: ChartConfig;
  chartType: "bar" | "pie";
  emptyMessage?: string;
  className?: string;
  height?: string;
  barChartProps?: Partial<
    Omit<BarChartProps, "data" | "dataKey" | "labelKey" | "chartConfig" | "emptyMessage" | "className" | "height">
  >;
  pieChartProps?: Partial<
    Omit<PieChartProps, "data" | "dataKey" | "nameKey" | "chartConfig" | "emptyMessage" | "className" | "height">
  >;
}

export function DistributionChart({
  data,
  dataKey,
  labelKey,
  chartConfig,
  chartType,
  emptyMessage = "No data available",
  className = "h-64",
  height,
  barChartProps = {},
  pieChartProps = {},
}: DistributionChartProps) {
  if (chartType === "pie") {
    return (
      <PieChart
        data={data}
        dataKey={dataKey}
        nameKey={labelKey}
        chartConfig={chartConfig}
        emptyMessage={emptyMessage}
        className={className}
        height={height}
        {...pieChartProps}
      />
    );
  }

  return (
    <BarChart
      data={data}
      dataKey={dataKey}
      labelKey={labelKey}
      chartConfig={chartConfig}
      emptyMessage={emptyMessage}
      className={className}
      height={height}
      {...barChartProps}
    />
  );
}
