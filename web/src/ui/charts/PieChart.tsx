// from hushbacks packages/ui/src/components/charts/PieChart.tsx
import { type ChartConfig, ChartContainer, ChartTooltip, ChartTooltipContent } from "@/ui/chart";
import { Cell, Pie, type PieProps, PieChart as RechartsPieChart, ResponsiveContainer } from "recharts";

export interface PieChartProps {
  data: Record<string, unknown>[];
  dataKey: string;
  nameKey?: string;
  chartConfig: ChartConfig;
  emptyMessage?: string;
  className?: string;
  height?: string;
  pieProps?: Partial<Omit<PieProps, "ref">>;
  showTooltip?: boolean;
  tooltipFormatter?: (value: unknown, name: string, props: unknown) => [string, string];
  showLabel?: boolean;
  labelFormatter?: (entry: unknown) => string;
  colors?: string[];
}

export function PieChart({
  data,
  dataKey,
  nameKey,
  chartConfig,
  emptyMessage = "No data available",
  className = "h-64",
  height,
  pieProps = {},
  showTooltip = true,
  tooltipFormatter,
  showLabel = false,
  labelFormatter,
  colors,
}: PieChartProps) {
  if (!data || data.length === 0) {
    return <div className={`flex items-center justify-center ${className} text-muted-foreground`}>{emptyMessage}</div>;
  }

  const getColorForIndex = (index: number, entry: Record<string, unknown>) => {
    if (colors && colors.length > 0) {
      return colors[index % colors.length];
    }
    if (nameKey && entry[nameKey]) {
      return `var(--color-${String(entry[nameKey])})`;
    }
    return `var(--color-${dataKey})`;
  };

  return (
    <ChartContainer config={chartConfig} className={className} style={height ? { height } : undefined}>
      <ResponsiveContainer width="100%" height="100%">
        <RechartsPieChart>
          {/* @ts-ignore - recharts type definitions issue with ref */}
          <Pie
            data={data}
            dataKey={dataKey}
            cx="50%"
            cy="50%"
            labelLine={false}
            outerRadius={80}
            fill="#8884d8"
            label={showLabel ? labelFormatter || ((entry) => entry[nameKey || dataKey]) : false}
            {...pieProps}
          >
            {data.map((entry, index) => (
              <Cell key={`cell-${index}`} fill={getColorForIndex(index, entry)} />
            ))}
          </Pie>
          {showTooltip && <ChartTooltip content={<ChartTooltipContent />} formatter={tooltipFormatter} />}
        </RechartsPieChart>
      </ResponsiveContainer>
    </ChartContainer>
  );
}
