import { useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { documentService, type Document } from "../api";
import { lanesForWidth } from "../lib/grid-lanes";
import { ConfirmDialog } from "./ConfirmDialog";
import { DocumentCard } from "./DocumentCard";
import { EmptyState, type EmptyReason } from "./EmptyState";

/**
 * Below this many loaded documents the plain grid is kept: virtualization
 * only pays off once the DOM itself becomes the bottleneck, and small
 * libraries keep the original markup verbatim.
 */
const VIRTUALIZE_AFTER = 60;
/** Rough card height until the first real measurement lands. */
const ROW_ESTIMATE = 320;

function cssNumber(variable: string, fallback: number): number {
  if (typeof window === "undefined") return fallback;
  const value = Number.parseFloat(
    getComputedStyle(document.documentElement).getPropertyValue(variable),
  );
  return Number.isFinite(value) ? value : fallback;
}

/**
 * How many cards the CSS grid fits per row. Read from the same variables the
 * stylesheet uses (`--card-min`, `--ui-scale`, 1rem gap), measured against
 * the real content width so drawer/resize changes regroup the rows.
 *
 * An off-by-one against `auto-fill` is harmless: each rendered row is its own
 * grid with exactly `lanes` children, so the count only affects card width,
 * never correctness.
 */
function useGridLanes(gridRef: RefObject<HTMLDivElement | null>): number {
  const [lanes, setLanes] = useState(4);
  useEffect(() => {
    const el = gridRef.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const compute = () => {
      const minCard = cssNumber("--card-min", 140) * cssNumber("--ui-scale", 1);
      const gap = cssNumber("font-size", 16);
      setLanes(lanesForWidth(el.clientWidth, minCard, gap));
    };
    compute();
    const observer = new ResizeObserver(compute);
    observer.observe(el);
    return () => observer.disconnect();
  }, [gridRef]);
  return lanes;
}

interface DocumentGridProps {
  documents: Document[];
  total: number;
  hasMore: boolean;
  isLoading: boolean;
  isLoadingMore: boolean;
  loadError: string | null;
  /** Why the grid is empty, when it is. Never read while there are documents. */
  emptyReason: EmptyReason;
  onOpenDocument: (doc: Document) => void;
  onOpenDetails: (doc: Document) => void;
  onMove: (doc: Document) => void;
  onRefresh: () => void;
  onLoadMore: () => void;
  onRetry: () => void;
  onSearchEverywhere: () => void;
  onImport: () => void;
}

export function DocumentGrid({
  documents,
  total,
  hasMore,
  isLoading,
  isLoadingMore,
  loadError,
  emptyReason,
  onOpenDocument,
  onOpenDetails,
  onMove,
  onRefresh,
  onLoadMore,
  onRetry,
  onSearchEverywhere,
  onImport,
}: DocumentGridProps) {
  const [pendingDelete, setPendingDelete] = useState<Document | null>(null);

  const scrollRef = useRef<HTMLDivElement>(null);
  const gridRef = useRef<HTMLDivElement>(null);
  const lanes = useGridLanes(gridRef);
  const virtualized = documents.length > VIRTUALIZE_AFTER;

  const rows = useMemo(() => {
    if (!virtualized) return [];
    const grouped: Document[][] = [];
    for (let i = 0; i < documents.length; i += lanes) {
      grouped.push(documents.slice(i, i + lanes));
    }
    return grouped;
  }, [documents, lanes, virtualized]);

  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_ESTIMATE,
    lanes,
    overscan: 4,
    measureElement: (el) => el.getBoundingClientRect().height,
  });

  const confirmDelete = async () => {
    if (!pendingDelete) return;
    const doc = pendingDelete;
    setPendingDelete(null);
    try {
      await documentService.remove(doc.id);
      onRefresh();
    } catch (err) {
      console.error("Failed to delete document", err);
    }
  };

  const toggleFavorite = async (doc: Document) => {
    try {
      await documentService.setFavorite(doc.id, !doc.isFavorite);
      onRefresh();
    } catch (err) {
      console.error("Failed to update favorite", err);
    }
  };

  const renderCard = (doc: Document) => (
    <DocumentCard
      key={doc.id}
      document={doc}
      onOpen={() => onOpenDocument(doc)}
      onOpenDetails={() => onOpenDetails(doc)}
      onToggleFavorite={() => toggleFavorite(doc)}
      onMove={() => onMove(doc)}
      onDelete={() => setPendingDelete(doc)}
    />
  );

  return (
    <div className="document-grid-container" ref={scrollRef}>
      <ConfirmDialog
        isOpen={pendingDelete !== null}
        title="Delete document"
        message={
          pendingDelete
            ? `"${pendingDelete.title}" and its file will be permanently removed. This cannot be undone.`
            : ""
        }
        confirmLabel="Delete"
        danger
        onConfirm={confirmDelete}
        onCancel={() => setPendingDelete(null)}
      />

      {isLoading && documents.length === 0 ? (
        <div className="document-grid-status" role="status">Loading documents...</div>
      ) : loadError && documents.length === 0 ? (
        <div className="document-grid-status" role="alert">
          <p>{loadError}</p>
          <button type="button" className="btn btn-primary" onClick={onRetry}>Try again</button>
        </div>
      ) : documents.length === 0 ? (
        <EmptyState
          reason={emptyReason}
          onSearchEverywhere={onSearchEverywhere}
          onImport={onImport}
        />
      ) : (
        <>
          {virtualized ? (
            <div
              ref={gridRef}
              style={{
                position: "relative",
                width: "100%",
                height: `${rowVirtualizer.getTotalSize()}px`,
              }}
            >
              {rowVirtualizer.getVirtualItems().map((row) => (
                <div
                  key={row.index}
                  data-index={row.index}
                  ref={(node) => {
                    if (node) rowVirtualizer.measureElement(node);
                  }}
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    transform: `translateY(${row.start}px)`,
                  }}
                >
                  <div className="document-grid">
                    {rows[row.index]?.map((doc) => renderCard(doc))}
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div className="document-grid">
              {documents.map((doc) => renderCard(doc))}
            </div>
          )}
          <div className="document-grid-pagination">
            <p className="document-grid-count" aria-live="polite">
              Showing {documents.length} of {total} {total === 1 ? "document" : "documents"}
            </p>
            {loadError && <p className="document-grid-load-error" role="alert">{loadError}</p>}
            {hasMore && (
              <button
                type="button"
                className="btn btn-ghost document-grid-load-more"
                onClick={loadError ? onRetry : onLoadMore}
                disabled={isLoadingMore}
                aria-busy={isLoadingMore}
              >
                {isLoadingMore ? "Loading..." : loadError ? "Try again" : "Load more"}
              </button>
            )}
          </div>
        </>
      )}
    </div>
  );
}
