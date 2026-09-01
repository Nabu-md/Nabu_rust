// ──────────────────────────────────────────────────────────────────────────────
// note_view.tsx — read-only markdown presentation (live preview in editor)
//
// Mirrors: ui-react/src/components/note_view.rs (NoteView)
//
// Renders the raw markdown source text with `whitespace-pre-wrap` so formatting
// is visible without a markdown-to-HTML renderer. This preserves the existing
// LePtOS / Dioxus behaviour exactly — a full markdown rendering pipeline is
// a future enhancement.
// ──────────────────────────────────────────────────────────────────────────────

export interface NoteViewProps {
  /** The markdown content to display (read-only preview). */
  content: string;
}

/**
 * Read-only preview of a note's content.
 *
 * Renders the raw markdown source. A markdown renderer was originally
 * referenced here (`nabu_core::parser::parse_markdown_to_html`), but no such
 * module exists in nabu-core, so the view renders the source text directly
 * until a renderer is introduced.
 */
export function NoteView({ content }: NoteViewProps) {
  return (
    <div className="note-view whitespace-pre-wrap text-gray-300 text-sm leading-relaxed">
      {content}
    </div>
  );
}
