import { onUnmounted, ref } from 'vue';
import { TransferRate } from './transferRate';

export { formatRate } from './transferRate';

export const useTransferRate = (readBytes: () => number) => {
  const rate = ref(0);
  const meter = new TransferRate();
  let timer: ReturnType<typeof setInterval> | undefined;
  const stop = () => { if (timer) clearInterval(timer); timer = undefined; };
  const start = () => {
    stop();
    rate.value = 0;
    meter.reset(readBytes(), performance.now());
    timer = setInterval(() => { rate.value = meter.sample(readBytes(), performance.now()); }, 500);
  };
  onUnmounted(stop);
  return { rate, start, stop };
};
