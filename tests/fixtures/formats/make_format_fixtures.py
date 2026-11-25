# Same old/new loan dataset in every supported container, from independent writers
# (csv, json, pyarrow, openpyxl). tests/formats.rs asserts the tool gives the same verdict
# and the same comparison for every pairing. (shape: dup.* has a repeated key.)
#   uv run --with pyarrow --with openpyxl python make_format_fixtures.py .
import csv, json, sys
import openpyxl, pyarrow as pa, pyarrow.parquet as pq

out = sys.argv[1]
hdr = ["loan_id", "region", "balance", "rate"]
old = [("L1", "East", 1000.5, 0.05), ("L2", "West", 2000.0, 0.06), ("L3", "North", 300.25, 0.04)]
new = [("L1", "East", 1000.5, 0.05), ("L2", "West", 2150.0, 0.06), ("L3", "North", 300.25, 0.045)]

def txt(v):
    return str(int(v)) if isinstance(v, float) and v == int(v) else str(v)

# Same columns and types, but loan_id L2 appears twice: key is not unique (INCOMPATIBLE).
dup = [("L1", "East", 1000.5, 0.05), ("L2", "West", 2000.0, 0.06), ("L2", "West", 2150.0, 0.06)]

for tag, rows in (("old", old), ("new", new), ("dup", dup)):
    for ext, delim in (("csv", ","), ("tsv", "\t")):
        with open(f"{out}/{tag}.{ext}", "w", newline="") as f:
            w = csv.writer(f, delimiter=delim, lineterminator="\n")
            w.writerow(hdr)
            for r in rows:
                w.writerow([txt(x) for x in r])
    with open(f"{out}/{tag}.jsonl", "w") as f:
        for r in rows:
            f.write(json.dumps(dict(zip(hdr, r))) + "\n")
    schema = pa.schema([("loan_id", pa.string()), ("region", pa.string()),
                        ("balance", pa.float64()), ("rate", pa.float64())])
    pq.write_table(pa.table({h: [r[i] for r in rows] for i, h in enumerate(hdr)}, schema=schema),
                   f"{out}/{tag}.parquet")
    wb = openpyxl.Workbook(); ws = wb.active; ws.title = "loans"; ws.append(hdr)
    for r in rows:
        ws.append(list(r))
    wb.save(f"{out}/{tag}.xlsx")

# A workbook with two sheets: --sheet is required; the data sits on "loans".
wb = openpyxl.Workbook(); ws = wb.active; ws.title = "notes"; ws.append(["scratch"])
ws2 = wb.create_sheet("loans"); ws2.append(hdr)
for r in new:
    ws2.append(list(r))
wb.save(f"{out}/new_two_sheets.xlsx")

# Parquet with a binary column: refused by the format reader.
pq.write_table(pa.table({"loan_id": ["L1"], "blob": pa.array([b"secret"], pa.binary())}),
               f"{out}/binary.parquet")
# Not really a parquet file.
open(f"{out}/not_really.parquet", "w").write("loan_id,balance\nL1,1\n")
