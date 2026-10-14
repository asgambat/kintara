import React, { useEffect, useState, useRef, useCallback } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { codeToHtml } from "shiki";
import { annotationService, documentService, type Annotation } from "../api";
import { onHighlightRequest, reportHighlight } from "../lib/reader-events";
import { safeHighlightColor } from "../lib/highlight-color";
import "./MarkdownReader.css";

interface MarkdownReaderProps {
  documentId: number;
}

/** Read the current --highlight-color CSS variable from the document root. */
const getHighlightColor = () =>
  getComputedStyle(document.documentElement)
    .getPropertyValue('--highlight-color')
    .trim() || "rgba(139, 92, 246, 0.35)";

/** Upper bound on highlight DOM work per render, so a huge annotation list
 *  cannot freeze the reader. */
const MAX_HIGHLIGHT_MARKS = 1000;

/** Longest annotation excerpt eligible for in-text highlighting. */
const MAX_HIGHLIGHT_EXCERPT = 2000;

function unwrapMarks(root: HTMLElement) {
  root.querySelectorAll("mark[data-annotation-id]").forEach((mark) => {
    const parent = mark.parentNode;
    if (!parent) return;
    parent.replaceChild(document.createTextNode(mark.textContent ?? ""), mark);
  });
}

function collectTextNodes(root: Node): Text[] {
  const out: Text[] = [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      const parent = node.parentNode;
      if (!parent || !node.textContent) return NodeFilter.FILTER_REJECT;
      if (
        parent instanceof HTMLElement &&
        (parent.closest("mark[data-annotation-id]") ||
          parent.closest("script,style"))
      ) {
        return NodeFilter.FILTER_REJECT;
      }
      return NodeFilter.FILTER_ACCEPT;
    },
  });
  while (walker.nextNode()) out.push(walker.currentNode as Text);
  return out;
}

/**
 * Wraps annotation excerpts in `<mark>` elements built with the DOM API.
 *
 * Everything user-controlled (excerpt text, color, id) travels through
 * `textContent`, `dataset` and `style` — never through HTML parsing — so a
 * crafted annotation cannot break out into script. Raw HTML in the document
 * itself is inert because the Markdown pipeline below no longer parses it.
 */
function applyHighlights(root: HTMLElement, annotations: Annotation[]) {
  unwrapMarks(root);
  let budget = MAX_HIGHLIGHT_MARKS;
  for (const ann of annotations) {
    const needle = ann.content ?? "";
    if (needle.length === 0 || needle.length > MAX_HIGHLIGHT_EXCERPT) continue;
    const color = safeHighlightColor(ann.color);
    for (const node of collectTextNodes(root)) {
      if (budget <= 0) break;
      let current: Text | null = node;
      while (current && budget > 0) {
        const haystack = current.textContent ?? "";
        const idx = haystack.indexOf(needle);
        if (idx === -1) break;
        const matched = current.splitText(idx);
        const rest = matched.splitText(needle.length);
        const mark = document.createElement("mark");
        mark.dataset.annotationId = String(ann.id);
        mark.textContent = needle;
        mark.style.backgroundColor = color;
        mark.style.cursor = "pointer";
        mark.style.borderRadius = "2px";
        mark.title = "Click to remove highlight";
        matched.parentNode?.replaceChild(mark, matched);
        current = rest;
        budget -= 1;
      }
      if (budget <= 0) break;
    }
    if (budget <= 0) break;
  }
}

export const MarkdownReader: React.FC<MarkdownReaderProps> = ({ documentId }) => {
  const [content, setContent] = useState<string>("");
  const [error, setError] = useState<string | null>(null);
  const [annotations, setAnnotations] = useState<Annotation[]>([]);
  /** Shown when an accepted passage is not present in the loaded source. */
  const [placementNotice, setPlacementNotice] = useState<string | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const loadAnnotations = useCallback(async () => {
    try {
      setAnnotations(await documentService.annotations(documentId));
    } catch (err) {
      console.error("Failed to load annotations:", err);
    }
  }, [documentId]);

  useEffect(() => {
    const loadFile = async () => {
      try {
        setContent(await documentService.text(documentId));
        loadAnnotations();
      } catch (err) {
        console.error("Failed to read markdown file:", err);
        setError("Failed to load document.");
      }
    };
    loadFile();
  }, [documentId]);

  /**
   * Places an accepted AI passage as a highlight.
   *
   * Markdown extracts to a single page of the file's own source, so the quote
   * should be a literal slice of what is loaded here. It is checked anyway: a
   * document edited on disk since it was indexed would otherwise store a
   * highlight that never renders.
   */
  useEffect(() => {
    return onHighlightRequest(documentId, async ({ excerpt }) => {
      setPlacementNotice(null);
      if (!content.includes(excerpt)) {
        setPlacementNotice("That passage is not in the current version of this document.");
        reportHighlight({ documentId, excerpt, placed: false });
        return;
      }
      try {
        await annotationService.create({
          documentId,
          annotationType: "highlight",
          serializedPosition: "text_match",
          content: excerpt,
          color: getHighlightColor(),
        });
        await loadAnnotations();
        reportHighlight({ documentId, excerpt, placed: true });
      } catch (err) {
        console.error("Failed to save annotation", err);
        setPlacementNotice("That highlight could not be saved.");
        reportHighlight({ documentId, excerpt, placed: false });
      }
    });
  }, [documentId, content, loadAnnotations]);

  /** On mouseup — immediately highlight selected text, no confirmation dialog. */
  const handleTextSelection = async () => {
    const selection = window.getSelection();
    if (!selection || selection.isCollapsed) return;

    const text = selection.toString().trim();
    if (text.length < 3) return;

    const color = getHighlightColor();

    try {
      await annotationService.create({
        documentId,
        annotationType: "highlight",
        serializedPosition: "text_match",
        content: text,
        color,
      });
      await loadAnnotations();
    } catch (err) {
      console.error("Failed to save annotation", err);
    }
    selection.removeAllRanges();
  };

  /** Click on a <mark> element → remove that annotation. */
  const handleMarkClick = useCallback(async (e: MouseEvent) => {
    const target = e.target as HTMLElement;
    if (target.tagName !== "MARK") return;

    const annotationId = Number(target.dataset.annotationId);
    if (!annotationId) return;

    e.stopPropagation();
    try {
      await annotationService.remove(annotationId);
      await loadAnnotations();
    } catch (err) {
      console.error("Failed to delete annotation", err);
    }
  }, [loadAnnotations]);

  // Attach click listener on the container to catch mark clicks via delegation
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    el.addEventListener("click", handleMarkClick);
    return () => el.removeEventListener("click", handleMarkClick);
  }, [handleMarkClick]);

  // Paint highlights after each render of content or annotations.
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    applyHighlights(el, annotations);
  });

  const processContent = (raw: string) => {
    let processed = raw;

    // Fix missing blank line before tables
    processed = processed.replace(/([^\n])\n(\s*\|.*?\|\s*\n\s*\|[-:\s|]+\|\s*(\n|$))/g, '$1\n\n$2');
    // Fix empty lines between table rows
    processed = processed.replace(/\|\s*\n\s*\n\s*\|/g, '|\n|');

    // WikiLinks [[Link]] → plain "#Link" text. There is no wikilink target to
    // navigate to, and emitting an <a> tag would need raw-HTML rendering.
    processed = processed.replace(/\[\[(.*?)\]\]/g, (_, target: string) => `#${target}`);

    return processed;
  };

  if (error) {
    return <div className="reader-error">{error}</div>;
  }

  return (
    <div className="markdown-reader-container relative" ref={containerRef} onMouseUp={handleTextSelection}>
      {placementNotice && (
        <p className="pdf-placement-notice" role="status">
          {placementNotice}
          <button
            type="button"
            className="pdf-placement-dismiss"
            onClick={() => setPlacementNotice(null)}
            aria-label="Dismiss"
          >
            Dismiss
          </button>
        </p>
      )}
      {/*
        No rehypeRaw on purpose: raw HTML in library documents renders as
        inert text instead of live elements, so a hostile .md on a shared
        volume cannot run script in another reader's browser. Highlights are
        painted afterwards with DOM APIs (applyHighlights), never with
        HTML string interpolation.
      */}
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          code({ node, inline, className, children, ...props }: any) {
            const match = /language-(\w+)/.exec(className || "");
            const [highlightedCode, setHighlightedCode] = useState<string | null>(null);

            useEffect(() => {
              if (!inline && match) {
                // Shiki escapes the code it highlights, so this HTML is
                // generated from text — not from document markup.
                codeToHtml(String(children).replace(/\n$/, ""), {
                  lang: match[1],
                  theme: "github-dark",
                }).then(setHighlightedCode, () => undefined);
              }
            }, [match, children, inline]);

            if (!inline && match) {
              return highlightedCode ? (
                <div dangerouslySetInnerHTML={{ __html: highlightedCode }} />
              ) : (
                <pre className={className} {...props}>
                  <code>{children}</code>
                </pre>
              );
            }
            return (
              <code className={`${className} inline-code`} {...props}>
                {children}
              </code>
            );
          }
        }}
      >
        {processContent(content)}
      </ReactMarkdown>
    </div>
  );
};
