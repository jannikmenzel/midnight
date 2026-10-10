"""Builds postal_codes.tsv from the GeoNames postal code dumps (CC BY 4.0).

Download https://download.geonames.org/export/zip/{DE,AT,CH}.zip, unzip next to this
script and run `python3 build_postal_codes.py`. The app only reads the generated TSV,
it never goes online.
"""
import pathlib

HERE = pathlib.Path(__file__).parent
rows = set()
for country in ("DE", "AT", "CH"):
    for line in (HERE / f"{country}.txt").read_text(encoding="utf-8").splitlines():
        f = line.split("\t")
        zip_code, city, state, accuracy = f[1], f[2].strip(), f[3], f[11]
        if not accuracy:
            continue
        if country == "AT" and city.startswith("Wien,"):
            city = "Wien"
        rows.add((country, zip_code, city, state))

out = HERE / "postal_codes.tsv"
with out.open("w", encoding="utf-8") as fh:
    for r in sorted(rows):
        fh.write("\t".join(r) + "\n")
print(f"{len(rows)} rows -> {out.name}")
