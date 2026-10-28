import { describe, expect, it, vi, beforeEach } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router'
import type { ReactNode } from 'react'
import { RoomController } from '../room/controller'
import { RoomContext } from '../room/context'
import { ToastProvider } from '../components/Toast'
import { ChatPanel } from '../features/chat/ChatPanel'
import { QueuePanel } from '../features/queue/QueuePanel'
import { JoinGate } from '../pages/JoinGate'
import type { ClientEvent } from '../realtime/protocol'
import { guest, host, room, sync } from './fixtures'

function setup(selfId = host.id) {
  const ctrl = new RoomController('abcdefghijkm', 'pts_test')
  const sent: ClientEvent[] = []
  vi.spyOn(ctrl.socket, 'send').mockImplementation((e) => {
    sent.push(e)
    return true
  })
  ctrl.store.dispatch({ type: 'server', event: sync(selfId) })
  ctrl.store.dispatch({ type: 'status', status: 'open' })
  const wrap = (ui: ReactNode) => (
    <MemoryRouter>
      <ToastProvider>
        <RoomContext.Provider value={ctrl}>{ui}</RoomContext.Provider>
      </ToastProvider>
    </MemoryRouter>
  )
  return { ctrl, sent, wrap }
}

describe('ChatPanel', () => {
  beforeEach(() => {
    Element.prototype.scrollTo = vi.fn()
  })

  it('shows an empty state, sends on Enter, and renders the optimistic message', async () => {
    const { sent, wrap } = setup()
    render(wrap(<ChatPanel />))
    expect(screen.getByText('No messages yet.')).toBeInTheDocument()

    const box = screen.getByLabelText('Message')
    await userEvent.type(box, 'hello room{Enter}')
    expect(sent).toEqual([expect.objectContaining({ type: 'chat_send', body: 'hello room' })])
    expect(screen.getByText('hello room')).toBeInTheDocument()
    expect(box).toHaveValue('')
  })

  it('keeps Shift+Enter as a newline and disables sending when empty', async () => {
    const { sent, wrap } = setup()
    render(wrap(<ChatPanel />))
    expect(screen.getByRole('button', { name: 'Send message' })).toBeDisabled()
    await userEvent.type(screen.getByLabelText('Message'), 'line one{Shift>}{Enter}{/Shift}line two')
    expect(sent).toHaveLength(0)
    expect(screen.getByLabelText('Message')).toHaveValue('line one\nline two')
  })

  it('explains when the connection is down', () => {
    const { ctrl, wrap } = setup()
    ctrl.store.dispatch({ type: 'status', status: 'reconnecting' })
    render(wrap(<ChatPanel />))
    expect(screen.getByText(/Reconnecting/)).toBeInTheDocument()
    expect(screen.getByLabelText('Message')).toHaveAttribute('placeholder', 'Waiting for connection')
  })
})

describe('QueuePanel', () => {
  it('rejects links that are not YouTube before sending', async () => {
    const { sent, wrap } = setup()
    render(wrap(<QueuePanel />))
    await userEvent.type(screen.getByLabelText('Search YouTube or paste a link'), 'https://vimeo.com/123')
    await userEvent.click(screen.getByRole('button', { name: 'Add' }))
    expect(screen.getByRole('alert')).toHaveTextContent('paste a YouTube link')
    expect(sent).toHaveLength(0)
  })

  it('sends valid links and shows what is playing', async () => {
    const { sent, wrap } = setup()
    render(wrap(<QueuePanel />))
    expect(screen.getByText('A video')).toBeInTheDocument()
    await userEvent.type(screen.getByLabelText('Search YouTube or paste a link'), 'https://youtu.be/9bZkp7q19f0')
    await userEvent.click(screen.getByRole('button', { name: 'Add' }))
    expect(sent[0]).toMatchObject({ type: 'queue_add', url: 'https://youtu.be/9bZkp7q19f0' })
  })

  it('hides the form from guests when only the host may add', () => {
    const { ctrl, wrap } = setup(guest.id)
    const s = ctrl.store.getState()
    ctrl.store.dispatch({
      type: 'server',
      event: { type: 'room_settings_changed', room: { ...s.room!, settings: { control_mode: 'host_only', queue_mode: 'host_only', permanent: false } } },
    })
    render(wrap(<QueuePanel />))
    expect(screen.getByText('Only the host can add videos in this room.')).toBeInTheDocument()
    expect(screen.queryByLabelText('Search YouTube or paste a link')).not.toBeInTheDocument()
  })

  it('searches YouTube through the server and queues a picked result', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          results: [
            { video_id: 'aqz-KE-bpKQ', title: 'Big Buck Bunny', channel: 'Blender', thumbnail_url: 'https://i.ytimg.com/vi/aqz-KE-bpKQ/mqdefault.jpg', duration: 634, is_live: false },
          ],
        }),
        { status: 200, headers: { 'Content-Type': 'application/json' } },
      ),
    )
    vi.stubGlobal('fetch', fetchMock)
    const { sent, wrap } = setup()
    render(wrap(<QueuePanel />))
    await userEvent.type(screen.getByRole('combobox'), 'big buck bunny')
    const option = await screen.findByRole('option', { name: /Big Buck Bunny/ }, { timeout: 2000 })
    expect(fetchMock).toHaveBeenCalledTimes(1)
    const [url, init] = fetchMock.mock.calls[0]!
    expect(url).toBe('/api/rooms/abcdefghijkm/search?q=big%20buck%20bunny')
    expect(init.headers.Authorization).toBe('Bearer pts_test')
    expect(screen.getByText('10:34')).toBeInTheDocument()

    await userEvent.click(option)
    expect(sent.at(-1)).toMatchObject({ type: 'queue_add', url: 'https://www.youtube.com/watch?v=aqz-KE-bpKQ' })
    vi.unstubAllGlobals()
  })
})

describe('JoinGate', () => {
  it('validates the name and calls join', async () => {
    localStorage.clear()
    const onJoin = vi.fn().mockResolvedValue(undefined)
    render(
      <MemoryRouter>
        <JoinGate
          slug="abcdefghijkm"
          loading={false}
          summary={{ room: { ...room, settings: { ...room.settings } }, participant_count: 2, is_full: false }}
          onJoin={onJoin}
        />
      </MemoryRouter>,
    )
    expect(screen.getByText("Ana's room")).toBeInTheDocument()
    expect(screen.getByText('2 people are here right now.')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Join the room' }))
    expect(screen.getByRole('alert')).toHaveTextContent('Enter a name')
    expect(onJoin).not.toHaveBeenCalled()

    await userEvent.type(screen.getByLabelText('Your name'), '  Budi ')
    await userEvent.click(screen.getByRole('button', { name: 'Join the room' }))
    await waitFor(() => expect(onJoin).toHaveBeenCalledWith('Budi', undefined))
  })

  it('asks for a 4-digit PIN in protected rooms', async () => {
    localStorage.clear()
    const onJoin = vi.fn().mockResolvedValue(undefined)
    render(
      <MemoryRouter>
        <JoinGate slug="abcdefghijkm" loading={false} summary={{ room: { ...room, has_password: true, settings: { ...room.settings } }, participant_count: 1, is_full: false }} onJoin={onJoin} />
      </MemoryRouter>,
    )
    await userEvent.type(screen.getByLabelText('Your name'), 'Budi')
    const pin = screen.getByLabelText('Room PIN')
    await userEvent.type(pin, '4a2b0c9')
    expect(pin).toHaveValue('4209')
    await userEvent.click(screen.getByRole('button', { name: 'Join the room' }))
    await waitFor(() => expect(onJoin).toHaveBeenCalledWith('Budi', '4209'))
  })

  it('blocks joining a full room', () => {
    render(
      <MemoryRouter>
        <JoinGate slug="abcdefghijkm" loading={false} summary={{ room: { ...room, settings: { ...room.settings } }, participant_count: 16, is_full: true }} onJoin={vi.fn()} />
      </MemoryRouter>,
    )
    expect(screen.getByRole('button', { name: 'Room is full' })).toBeDisabled()
  })
})
