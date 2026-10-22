// from hushbacks packages/ui/src/components/charts/BarChart.tsx
import {
  Bar,
  type BarProps,
  CartesianGrid,
  LabelList,
  BarChart as RechartsBarChart,
  XAxis,
  type XAxisProps,
  YAxis,
  type YAxisProps,
} from "recharts";
import type { Formatter } from "recharts/types/component/DefaultTooltipContent";

import { type ChartConfig, ChartContainer, ChartTooltip, ChartTooltipContent } from "@/ui/chart";

export interface BarChartProps {
  data: Record<string, unknown>[];
  dataKey: string;
  labelKey?: string;
  chartConfig: ChartConfig;
  layout?: "vertical" | "horizontal";
  emptyMessage: string;
  tooltipFormatter?: Formatter<number | string, string>;
  xAxisProps?: XAxisProps;
  yAxisProps?: YAxisProps;
  className?: string;
  height?: string;
  barProps?: BarProps;
}

function BarChart({
  data,
  dataKey,
  labelKey,
  chartConfig,
  className,
  layout = "vertical",
  height,
  tooltipFormatter,
  xAxisProps,
  yAxisProps,
  barProps,
}: BarChartProps) {
  const isHorizontal = layout === "horizontal";

  const getColor = () => `var(--color-${dataKey})`;

  const defaultXAxisProps = isHorizontal
    ? {
        type: "number" as const,
        tickLine: false,
        axisLine: false,
        ...xAxisProps,
      }
    : {
        tickLine: false,
        axisLine: false,
        tickMargin: 8,
        ...xAxisProps,
      };

  const defaultYAxisProps = isHorizontal
    ? {
        type: "category" as const,
        dataKey: labelKey,
        tickLine: false,
        axisLine: false,
        width: 150,
        ...yAxisProps,
      }
    : {
        tickLine: false,
        axisLine: false,
        ...yAxisProps,
      };
  return (
    <ChartContainer config={chartConfig} className={className} style={height ? { height } : undefined}>
      <RechartsBarChart
        accessibilityLayer
        data={data}
        margin={{
          top: 20,
        }}
      >
        <CartesianGrid vertical={false} />
        {/* @ts-ignore - recharts type definitions issue with textAnchor */}
        <XAxis {...defaultXAxisProps} />
        <YAxis {...defaultYAxisProps} />
        <ChartTooltip cursor={false} formatter={tooltipFormatter} content={<ChartTooltipContent />} />
        {/* @ts-ignore - recharts type definitions issue with ref */}
        <Bar
          dataKey={dataKey}
          fill={getColor()}
          radius={isHorizontal ? [0, 4, 4, 0] : [4, 4, 0, 0]}
          {...(barProps as Omit<BarProps, "dataKey" | "ref">)}
        >
          <LabelList position="top" offset={12} className="fill-foreground" fontSize={12} />
        </Bar>
      </RechartsBarChart>
    </ChartContainer>
  );
}

export { BarChart };
