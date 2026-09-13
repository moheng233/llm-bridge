<template>
  <article
    data-slot="list-item"
    class="flex min-w-0 flex-col justify-between gap-3 rounded-md border bg-card p-4 md:flex-row md:items-start"
  >
    <div class="min-w-0 flex-1 space-y-2">
      <div class="flex min-w-0 flex-wrap items-center gap-2">
        <h2
          class="min-w-0 text-base leading-6 font-semibold break-all [&_a]:text-foreground [&_a:hover]:text-primary [&_a:hover]:underline [&_button]:text-left [&_button:hover]:text-primary [&_button:hover]:underline"
        >
          <slot name="title" />
        </h2>
        <slot name="status" />
      </div>
      <p v-if="$slots.subtitle" class="font-mono text-xs break-all text-muted-foreground">
        <slot name="subtitle" />
      </p>
      <div class="space-y-1 text-sm text-muted-foreground"><slot /></div>
    </div>
    <div v-if="$slots.actions" class="flex shrink-0 flex-wrap items-center gap-2">
      <slot name="actions" />
    </div>
  </article>
</template>
