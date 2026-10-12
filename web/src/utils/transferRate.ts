export class TransferRate {
  private samples: Array<{ time: number; bytes: number }> = [];

  reset(bytes: number, time: number) { this.samples = [{ bytes, time }]; }

  sample(bytes: number, time: number): number {
    const last = this.samples.at(-1);
    if (!last || bytes < last.bytes || time <= last.time) {
      this.reset(bytes, time);
      return 0;
    }
    this.samples.push({ bytes, time });
    // Keep one point preceding the three-second window to include quiet periods.
    while (this.samples.length > 2 && this.samples[1].time <= time - 3000) this.samples.shift();
    const first = this.samples[0];
    return Math.max(0, Math.round((bytes - first.bytes) * 1000 / (time - first.time)));
  }
}

export const formatRate = (bytes: number) => {
  if (bytes <= 0) return '0 B/s';
  const units = ['B/s', 'KB/s', 'MB/s', 'GB/s'];
  const index = Math.min(units.length - 1, Math.floor(Math.log(bytes) / Math.log(1024)));
  return `${(bytes / 1024 ** index).toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
};
