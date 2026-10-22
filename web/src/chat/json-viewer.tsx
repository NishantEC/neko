// from hushbacks packages/ui/src/components/json-viewer/index.tsx
// (changed: compact 24px rows; the row is a div so its copy button is not nested in a button)
import { ArrowRight, CaretDown, CaretUp, Check, Copy, DotsThree } from "@/ui/icons";
import { Button } from "@/ui/button";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/ui/tooltip";
import { cn } from "@/ui/lib/utils";
import * as React from "react";

type JsonViewerProps = {
  data: unknown;
  rootName?: string;
  defaultExpanded?: boolean;
  className?: string;
};

export function JsonViewer({ data, rootName = "root", defaultExpanded = true, className }: JsonViewerProps) {
  return (
    <TooltipProvider>
      <div className={cn("font-mono text-sm", className)}>
        <JsonNode name={rootName} data={data} isRoot={true} defaultExpanded={defaultExpanded} />
      </div>
    </TooltipProvider>
  );
}

type JsonNodeProps = {
  name: string;
  data: unknown;
  isRoot?: boolean;
  defaultExpanded?: boolean;
  level?: number;
};

function JsonNode({ name, data, isRoot = false, defaultExpanded = true, level = 0 }: JsonNodeProps) {
  const [isExpanded, setIsExpanded] = React.useState(defaultExpanded);
  const [isCopied, setIsCopied] = React.useState(false);

  const handleToggle = () => {
    setIsExpanded(!isExpanded);
  };

  const copyToClipboard = (e: React.MouseEvent) => {
    e.stopPropagation();
    navigator.clipboard.writeText(JSON.stringify(data, null, 2));
    setIsCopied(true);
    setTimeout(() => setIsCopied(false), 2000);
  };

  const dataType = data === null ? "null" : Array.isArray(data) ? "array" : typeof data;
  const isExpandable =
    data !== null && data !== undefined && !(data instanceof Date) && (dataType === "object" || dataType === "array");
  const itemCount = isExpandable && data !== null && data !== undefined ? Object.keys(data).length : 0;

  return (
    <div className={cn("pl-4 group/object", level > 0 && "border-l border-border")}>
      <Button
        variant="ghost"
        // A div, not a button: the row holds its own copy button, and a button in a button is invalid.
        render={<div role="button" tabIndex={0} />}
        onKeyDown={(e: React.KeyboardEvent) => {
          if (isExpandable && (e.key === "Enter" || e.key === " ")) {
            e.preventDefault();
            handleToggle();
          }
        }}
        className={cn(
          "flex h-6 w-full items-center justify-start gap-1 px-1 py-0 text-xs font-normal hover:bg-muted/50 rounded -ml-4 cursor-pointer group/property sm:h-6",
          isRoot && "text-primary font-semibold",
        )}
        onClick={isExpandable ? handleToggle : undefined}
      >
        {isExpandable ? (
          <div className="w-4 h-4 flex items-center justify-center">
            {isExpanded ? (
              <CaretDown className="h-3.5 w-3.5 text-muted-foreground" />
            ) : (
              <ArrowRight className="h-3.5 w-3.5 text-muted-foreground" />
            )}
          </div>
        ) : (
          <div className="w-4" />
        )}

        <span className="text-primary">{name}</span>

        <span className="text-muted-foreground">
          {isExpandable ? (
            <>
              {dataType === "array" ? "[" : "{"}
              {!isExpanded && (
                <span className="text-muted-foreground">
                  {" "}
                  {itemCount} {itemCount === 1 ? "item" : "items"} {dataType === "array" ? "]" : "}"}
                </span>
              )}
            </>
          ) : (
            ":"
          )}
        </span>

        {!isExpandable && <JsonValue data={data} />}

        {!isExpandable && <div className="w-3.5" />}

        <Button
          variant="ghost"
          type="button"
          onClick={copyToClipboard}
          className="ml-auto opacity-0 group-hover/property:opacity-100 hover:bg-muted p-1 rounded"
          title="Copy to clipboard"
        >
          {isCopied ? (
            <Check className="h-3.5 w-3.5 text-green-500" />
          ) : (
            <Copy className="h-3.5 w-3.5 text-muted-foreground" />
          )}
        </Button>
      </Button>

      {isExpandable && isExpanded && data !== null && data !== undefined && (
        <div className="pl-4">
          {Object.keys(data).map((key) => (
            <JsonNode
              key={key}
              name={dataType === "array" ? `${key}` : key}
              data={data[key as keyof typeof data] as unknown}
              level={level + 1}
              defaultExpanded={level < 1}
            />
          ))}
          <div className="text-muted-foreground pl-4 py-1">{dataType === "array" ? "]" : "}"}</div>
        </div>
      )}
    </div>
  );
}

// Update the JsonValue function to make the entire row clickable with an expand icon
function JsonValue({ data }: { data: unknown }) {
  const [isExpanded, setIsExpanded] = React.useState(false);
  const dataType = typeof data;
  const TEXT_LIMIT = 80; // Character limit before truncation

  if (data === null) {
    return <span className="text-rose-500">null</span>;
  }

  if (data === undefined) {
    return <span className="text-muted-foreground">undefined</span>;
  }

  if (data instanceof Date) {
    return <span className="text-purple-500">{data.toISOString()}</span>;
  }

  switch (dataType) {
    case "string":
      if (typeof data === "string" && data.length > TEXT_LIMIT) {
        return (
          <Button
            variant="ghost"
            type="button"
            className="text-emerald-500 flex-1 flex items-center relative group cursor-pointer"
            onClick={(e: React.MouseEvent) => {
              e.stopPropagation();
              setIsExpanded(!isExpanded);
            }}
          >
            {`"`}
            {isExpanded ? (
              <span className="inline-block max-w-full">{String(data)}</span>
            ) : (
              <Tooltip delayDuration={300}>
                <TooltipTrigger asChild>
                  <span className="inline-block max-w-full">{String(data).substring(0, TEXT_LIMIT)}...</span>
                </TooltipTrigger>
                <TooltipContent side="bottom" className="max-w-md text-xs p-2 break-words">
                  {String(data)}
                </TooltipContent>
              </Tooltip>
            )}
            {`"`}
            <div className="absolute right-0 top-1/2 -translate-y-1/2 translate-x-[calc(100%+4px)] opacity-0 group-hover:opacity-100 transition-opacity">
              {isExpanded ? (
                <CaretUp className="h-3.5 w-3.5 text-muted-foreground" />
              ) : (
                <DotsThree className="h-3.5 w-3.5 text-muted-foreground" />
              )}
            </div>
          </Button>
        );
      }
      return <span className="text-emerald-500">{`"${String(data)}"`}</span>;
    case "number":
      return <span className="text-amber-500">{String(data)}</span>;
    case "boolean":
      return <span className="text-blue-500">{String(data)}</span>;
    default:
      return <span>{String(data)}</span>;
  }
}
