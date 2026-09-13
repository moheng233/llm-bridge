import { type Ref } from "vue";

export function useListPagination<T>(items: Ref<T[]>, resetKey: Ref<unknown>, pageSize = 50) {
  const page = ref(1);
  watch(
    resetKey,
    () => {
      page.value = 1;
    },
    { flush: "sync" },
  );
  watch(
    () => items.value.length,
    (count) => {
      page.value = Math.min(page.value, Math.max(1, Math.ceil(count / pageSize)));
    },
    { flush: "sync" },
  );
  const visible = computed(() =>
    items.value.slice((page.value - 1) * pageSize, page.value * pageSize),
  );
  return { page, visible, pageSize };
}
