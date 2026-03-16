import { parse, parseTree, modify, applyEdits, type ParseError, printParseErrorCode } from 'jsonc-parser'

export function parseJsonWithComments<T = unknown>(input: string): T {
  const errors: ParseError[] = []
  const parsed = parse(input, errors, {
    disallowComments: false,
    allowTrailingComma: true,
    allowEmptyContent: false,
  })

  if (errors.length > 0) {
    const first = errors[0]
    throw new Error(`JSON 语法错误(${printParseErrorCode(first.error)})，位置 ${first.offset}`)
  }

  return parsed as T
}

export function applyJsoncModification(source: string, path: (string | number)[], value: unknown): string {
  const edits = modify(source, path, value, { formattingOptions: { tabSize: 2, insertSpaces: true } })
  return applyEdits(source, edits)
}

export function extractJsoncValueText(source: string, key: string): string | null {
  const tree = parseTree(source, undefined, { disallowComments: false, allowTrailingComma: true })
  if (!tree || tree.type !== 'object' || !tree.children) return null
  for (const prop of tree.children) {
    if (!prop.children || prop.children.length < 2) continue
    const keyNode = prop.children[0]
    if (keyNode.value === key) {
      const valueNode = prop.children[1]
      return source.substring(valueNode.offset, valueNode.offset + valueNode.length)
    }
  }
  return null
}
