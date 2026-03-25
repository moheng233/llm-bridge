import { constructTableHelper } from "@tanstack/table-core";
import { createTable } from "./createTable.svelte";
import type {
  RowData,
  Table,
  TableFeatures,
  TableHelperOptions,
  TableHelper_Core,
  TableOptions,
} from "@tanstack/table-core";

export type TableHelper<
  TFeatures extends TableFeatures,
  TData extends RowData = any,
> = Omit<TableHelper_Core<TFeatures>, "tableCreator"> & {
  createTable: (
    tableOptions: Omit<
      TableOptions<TFeatures, TData>,
      "_features" | "_rowModels"
    >,
  ) => Table<TFeatures, TData>;
};

export function createTableHelper<
  TFeatures extends TableFeatures,
  TData extends RowData = any,
>(
  tableHelperOptions: TableHelperOptions<TFeatures>,
): TableHelper<TFeatures, TData> {
  // Create a wrapper function that matches the expected signature
  const tableCreator = <TDataInner extends RowData>(
    tableOptions: Omit<
      TableOptions<TFeatures, TDataInner>,
      "_features" | "_rowModels"
    >,
    selector?: any,
  ): Table<TFeatures, TDataInner> => {
    // Merge the helper options with the table options
    const fullOptions = {
      ...tableHelperOptions,
      ...tableOptions,
    } as TableOptions<TFeatures, TDataInner>;
    return createTable(fullOptions, selector);
  };

  const tableHelper = constructTableHelper(tableCreator, tableHelperOptions);
  return {
    ...tableHelper,
    createTable: tableHelper.tableCreator,
  } as any;
}

// test

// type Person = {
//   firstName: string
//   lastName: string
//   age: number
// }

// const tableHelper = createTableHelper({
//   _features: { rowSelectionFeature: {} },
//   TData: {} as Person,
// })

// const columns = [
//   tableHelper.columnHelper.accessor('firstName', { header: 'First Name' }),
//   tableHelper.columnHelper.accessor('lastName', { header: 'Last Name' }),
//   tableHelper.columnHelper.accessor('age', { header: 'Age' }),
//   tableHelper.columnHelper.display({ header: 'Actions', id: 'actions' }),
// ] as Array<ColumnDef<typeof tableHelper.features, Person, unknown>>

// const data: Array<Person> = []

// tableHelper.createTable({
//   columns,
//   data,
// })
