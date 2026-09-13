import {
  type InjectionKey,
  type ShallowRef,
  inject,
  onBeforeUnmount,
  provide,
  shallowRef,
  watchEffect,
} from "vue";
import { useRoute } from "vue-router";

interface PageHeader {
  title: string;
  path: string;
  owner: symbol;
}

const pageHeaderKey: InjectionKey<ShallowRef<PageHeader | null>> = Symbol("page-header");

export function providePageHeader() {
  const header = shallowRef<PageHeader | null>(null);
  provide(pageHeaderKey, header);
  return header;
}

export function usePageHeader(title: () => string) {
  const header = inject(pageHeaderKey, null);
  const route = useRoute();
  const owner = Symbol("page-header-owner");
  if (!header) return;
  watchEffect(() => {
    header.value = { title: title(), path: route.path, owner };
  });
  onBeforeUnmount(() => {
    if (header.value?.owner === owner) header.value = null;
  });
}
