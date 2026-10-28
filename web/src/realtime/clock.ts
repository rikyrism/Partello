// Estimates the offset between this browser's clock and the server's clock
// (NTP-style): offset = server_time - (sent + rtt / 2). The sample with the
// lowest round trip is the most trustworthy, so we keep a small window and use
// the best one.

interface Sample {
  offset: number
  rtt: number
}

const WINDOW = 8

export class ServerClock {
  private samples: Sample[] = []
  private offset = 0

  /** Server "now" in Unix ms. */
  now(): number {
    return Date.now() + this.offset
  }

  get offsetMs(): number {
    return this.offset
  }

  get hasSamples(): boolean {
    return this.samples.length > 0
  }

  /** Seed from a server timestamp received without a round-trip measurement. */
  seed(serverTime: number): void {
    if (this.samples.length === 0) this.offset = serverTime - Date.now()
  }

  addSample(sentAt: number, serverTime: number, receivedAt: number): void {
    const rtt = Math.max(0, receivedAt - sentAt)
    this.samples.push({ offset: serverTime - (sentAt + rtt / 2), rtt })
    if (this.samples.length > WINDOW) this.samples.shift()
    const best = this.samples.reduce((a, b) => (b.rtt < a.rtt ? b : a))
    this.offset = best.offset
  }

  reset(): void {
    this.samples = []
  }
}
