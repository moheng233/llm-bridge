// 请求追踪状态徽标统一映射 — 全站共用（dashboard / traces 列表 / trace 详情），
// 保证同状态在任意页面展示一致（badge 一致性）。

export function statusBadgeFor(status: string): { label: string; cls: string } {
  switch (status) {
    case "success":
      return { label: "成功", cls: "text-primary border-primary/30 bg-primary/10" };
    case "error":
      return { label: "失败", cls: "text-destructive border-destructive/30 bg-destructive/10" };
    case "cancelled":
      return { label: "已取消", cls: "text-muted-foreground border-border bg-muted" };
    case "streaming":
      return { label: "进行中", cls: "text-chart-2 border-chart-2/30 bg-chart-2/10" };
    default:
      return { label: "等待中", cls: "text-chart-4 border-chart-4/30 bg-chart-4/10" };
  }
}
