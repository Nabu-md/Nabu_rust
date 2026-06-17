import React, { useState, useMemo } from 'react'
import { useAppContext } from '../App'

// ---------------------------------------------------------------------------
// TagsPanel
//
// Renders a collapsible list of all tags extracted from the vault, sorted by
// frequency (most-used first). Clicking a tag dispatches TAG_FILTER_TOGGLE to
// filter the file tree to only files that contain that tag.
// ---------------------------------------------------------------------------

export function TagsPanel(): React.JSX.Element {
  const { state, dispatch } = useAppContext()
  const [isExpanded, setIsExpanded] = useState(true)

  // Derive sorted tag list from tagIndex — memoised on tagIndex changes
  const tags = useMemo(
    () =>
      Array.from(state.tagIndex.entries())
        .map(([tag, paths]) => ({ tag, count: paths.size }))
        .sort((a, b) => b.count - a.count),
    [state.tagIndex]
  )

  return (
    <div className="tags-panel shrink-0">
      {/* Section header */}
      <button
        className="w-full flex items-center justify-between px-3 py-1.5 text-xs font-semibold
                   text-onyx-text-muted uppercase tracking-wide hover:text-onyx-text
                   transition-colors select-none"
        aria-expanded={isExpanded}
        onClick={() => setIsExpanded((prev) => !prev)}
      >
        <span>Tags</span>
        <span aria-hidden="true" className="text-onyx-text-faint">
          {isExpanded ? '▼' : '▶'}
        </span>
      </button>

      {/* Tag list */}
      {isExpanded && (
        <div className="px-2 pb-2">
          {state.tagIndex.size === 0 ? (
            <p className="px-1 py-1 text-xs text-onyx-text-faint italic">No tags found</p>
          ) : (
            <div className="flex flex-col gap-0.5">
              {tags.map(({ tag, count }) => {
                const isPressed = state.selectedTags.has(tag)
                return (
                  <button
                    key={tag}
                    role="button"
                    aria-pressed={isPressed}
                    onClick={() => dispatch({ type: 'TAG_FILTER_TOGGLE', payload: tag })}
                    className={[
                      'flex items-center justify-between w-full px-2 py-0.5 text-xs rounded',
                      'transition-colors text-left select-none',
                      isPressed
                        ? 'bg-onyx-accent/20 text-onyx-accent'
                        : 'text-onyx-text-muted hover:text-onyx-text hover:bg-onyx-bg-mute'
                    ].join(' ')}
                  >
                    <span className="truncate">#{tag}</span>
                    <span
                      className={[
                        'ml-1 shrink-0 tabular-nums',
                        isPressed ? 'text-onyx-accent/70' : 'text-onyx-text-faint'
                      ].join(' ')}
                    >
                      {count}
                    </span>
                  </button>
                )
              })}
            </div>
          )}
        </div>
      )}
    </div>
  )
}
