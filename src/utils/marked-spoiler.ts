import type { MarkedExtension, Token, Tokens } from 'marked'

/**
 * marked inline extension for `>!spoiler!<` (Telegram/Discord-style):
 * the content renders as an opaque bar that reveals on hover, or stays
 * revealed after a click (the host toggles the `md-spoiler-revealed` class
 * via event delegation).
 *
 * The delimiters `>!` / `!<` never appear in GFM output for normal text, so
 * the tokenizer only runs when `>!` is present (see `start`).
 */
export function markedSpoiler(): MarkedExtension {
  return {
    hooks: {
      // A line starting with `>!` would be eaten by the GFM blockquote
      // tokenizer before inline rules run; insert a zero-width space so the
      // spoiler delimiter survives to the inline phase.
      preprocess(md: string) {
        return md.replace(/(^|\n)([ \t]*)>!/g, '$1$2\u200B>!')
      },
    },
    extensions: [
      {
        name: 'spoiler',
        level: 'inline',
        start(src: string) {
          const i = src.indexOf('>!')
          return i < 0 ? undefined : i
        },
        tokenizer(src: string) {
          const match = /^>!([\s\S]+?)!</.exec(src)
          if (!match) return undefined
          const text = match[1]
          if (!text.trim()) return undefined
          return {
            type: 'spoiler',
            raw: match[0],
            text,
            tokens: this.lexer.inlineTokens(text),
          } as Tokens.Generic
        },
        renderer(token: Tokens.Generic) {
          const inner = this.parser.parseInline((token.tokens ?? []) as Token[])
          return `<span class="md-spoiler">${inner}</span>`
        },
      },
    ],
  }
}
