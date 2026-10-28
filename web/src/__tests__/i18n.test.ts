import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import { id } from '../i18n/id'
import { translate } from '../i18n'
import { GENRES } from '../pages/landing/genres'
import { TRENDING_GENRES } from '../pages/landing/trending'
import { describeChange } from '../room/store'

const SRC = join(__dirname, '..')
const CALL = /\b(?:t|translate|msg)\(\s*(?:'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)")/g

function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name)
    if (statSync(path).isDirectory()) return name === '__tests__' ? [] : sourceFiles(path)
    return /\.(ts|tsx)$/.test(name) && name !== 'id.ts' ? [path] : []
  })
}

describe('Indonesian translations', () => {
  it('cover every string marked for translation in the source', () => {
    const missing: string[] = []
    for (const file of sourceFiles(SRC)) {
      for (const m of readFileSync(file, 'utf8').matchAll(CALL)) {
        const text = (m[1] ?? m[2] ?? '').replace(/\\'/g, "'").replace(/\\"/g, '"')
        if (!(text in id)) missing.push(`${file.replace(SRC, '')}: ${text}`)
      }
    }
    expect(missing).toEqual([])
  })

  it('cover genre names, device names and chat notes', () => {
    for (const g of GENRES) expect(id[g.name], g.name).toBeTruthy()
    for (const name of TRENDING_GENRES) expect(id[name], name).toBeTruthy()
    expect(id.microphone).toBeTruthy()
    const base = { id: 'x', display_name: 'Nita', is_host: false, connected: true, joined_at: 0, voice: null }
    const notes = [
      ...describeChange(base, { ...base, is_host: true, voice: { muted: true, camera: true } }, false),
      ...describeChange(base, { ...base, is_host: true, voice: { muted: true, camera: true } }, true),
    ]
    for (const n of notes) expect(id[n.text], n.text).toBeTruthy()
  })

  it('keep placeholders intact', () => {
    for (const [en, idText] of Object.entries(id)) {
      const vars = (s: string) => (s.match(/\{\w+\}/g) ?? []).sort().join(',')
      expect(vars(idText), en).toBe(vars(en))
    }
  })

  it('interpolate and fall back to English', () => {
    expect(translate('{n} unread', { n: 3 }, 'id')).toBe('3 belum dibaca')
    expect(translate('{n} unread', { n: 3 }, 'en')).toBe('3 unread')
    expect(translate('Not in the dictionary', undefined, 'id')).toBe('Not in the dictionary')
  })
})
