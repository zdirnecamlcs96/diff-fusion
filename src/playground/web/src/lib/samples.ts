/// Sample data the "Reset to sample" button loads. Lifted verbatim from
/// the legacy app.js so the demo flow lights up the same set of policies
/// (owned_by, additive, state_machine, set_by_key with composite identity).

export const SAMPLE = {
  system_a_name: "erp",
  system_b_name: "inv",
  system_a: {
    status: "closed",
    seqNumber: 42,
    supplier: { _id: "sup-1", name: "Acme Co." },
    price: 120,
    qty_recv: 6,
    lineItems: [
      { externalId: "A-L1", sku: "SKU-100", uom: "BTL", qty: 12 },
      { externalId: "A-L2", sku: "SKU-100", uom: "BOX", qty: 2 },
      { externalId: "A-L9", sku: "SKU-300", uom: "BTL", qty: 3 },
    ],
  },
  system_b: {
    status: "closed",
    seqNumber: 42,
    supplier: { _id: "sup-1", name: "Acme Co." },
    price: 999,
    qty_recv: 7,
    items: [
      { internalId: "B-I1", sku: "SKU-100", uom: "BTL", qty: 10 },
      { internalId: "B-I2", sku: "SKU-100", uom: "CTN", qty: 2 },
    ],
  },
  schema: {
    cif_schema: {
      po_status: { type: "string", required: true },
      po_seq_number: { type: "number", required: true },
      supplier_id: { type: "string", required: true },
      price: { type: "number", required: true },
      qty_recv: { type: "number", required: true },
      items: {
        type: "array",
        required: false,
        element: {
          externalId: { type: "string", anchor: "a" },
          internalId: { type: "string", anchor: "b" },
          sku: { type: "string" },
          uom: { type: "string" },
          qty: { type: "number" },
        },
      },
    },
    transformations: {
      erp: {
        po_status: { source_path: "status", type: "string" },
        po_seq_number: { source_path: "seqNumber", type: "number" },
        supplier_id: { source_path: "supplier._id", type: "string" },
        price: { source_path: "price", type: "number" },
        qty_recv: { source_path: "qty_recv", type: "number" },
        items: {
          source_path: "lineItems",
          type: "array",
          element: {
            externalId: { source_path: "externalId", type: "string" },
            sku: { source_path: "sku", type: "string" },
            uom: { source_path: "uom", type: "string" },
            qty: { source_path: "qty", type: "number" },
          },
        },
      },
      inv: {
        po_status: { source_path: "status", type: "string" },
        po_seq_number: { source_path: "seqNumber", type: "number" },
        supplier_id: { source_path: "supplier._id", type: "string" },
        price: { source_path: "price", type: "number" },
        qty_recv: { source_path: "qty_recv", type: "number" },
        items: {
          source_path: "items",
          type: "array",
          element: {
            internalId: { source_path: "internalId", type: "string" },
            sku: { source_path: "sku", type: "string" },
            uom: { source_path: "uom", type: "string" },
            qty: { source_path: "qty", type: "number" },
          },
        },
      },
    },
  },
  policy: {
    per_field: {
      price: { kind: "owned_by", system: "erp" },
      qty_recv: { kind: "additive" },
      po_status: {
        kind: "state_machine",
        transitions: [
          { from: "open", to: "closed" },
          { from: "open", to: "cancelled" },
        ],
      },
      items: {
        kind: "set_by_key",
        identity: ["sku", "uom"],
        a_anchor: "externalId",
        b_anchor: "internalId",
        on_both_changed: "Union",
      },
    },
  },
  ancestor: {
    po_status: "open",
    po_seq_number: 42,
    supplier_id: "sup-1",
    price: 100,
    qty_recv: 5,
    items: [
      { sku: "SKU-100", uom: "BTL", qty: 10, externalId: "A-L1", internalId: "B-I1" },
      { sku: "SKU-100", uom: "CTN", qty: 2, externalId: "A-L2", internalId: "B-I2" },
    ],
  },
} as const;
