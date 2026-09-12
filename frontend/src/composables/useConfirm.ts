// 全局确认器：危险操作、角色变更和未保存草稿共用。
//
// 用法：
//   import { useConfirm } from '~/composables/useConfirm'
//   const confirm = useConfirm()
//   const ok = await confirm({
//     title: "确认删除",
//     description: "确定要删除模型 X 吗？该操作不可撤销。",
//     confirmText: "确认删除",
//     destructive: true,
//   })
//   if (ok) { ... }

export interface ConfirmOptions {
  title: string;
  description?: string;
  confirmText?: string;
  cancelText?: string;
  destructive?: boolean;
}

interface ConfirmState extends ConfirmOptions {
  resolve: (v: boolean) => void;
  open: boolean;
}

const current = ref<ConfirmState | null>(null);

export function useConfirm() {
  return function confirm(opts: ConfirmOptions): Promise<boolean> {
    if (current.value?.open) {
      current.value.resolve(false);
    }
    return new Promise<boolean>((resolve) => {
      let settled = false;
      const finish = (value: boolean) => {
        if (settled) return;
        settled = true;
        if (current.value?.resolve === finish) current.value = null;
        resolve(value);
      };
      current.value = { ...opts, resolve: finish, open: true };
    });
  };
}

export function getConfirmState() {
  return current;
}
