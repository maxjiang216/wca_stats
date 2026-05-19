import requests
import zipfile
import io
import os
import sys


def main(output_dir, chunk_size=8192):
    url = "https://www.worldcubeassociation.org/export/results/v2/tsv"
    print(f"Downloading from {url} ...")

    with requests.get(url, stream=True) as r:
        r.raise_for_status()
        total = int(r.headers.get("content-length", 0))
        downloaded = 0
        last_pct = -1
        chunks = []
        for chunk in r.iter_content(chunk_size=chunk_size):
            chunks.append(chunk)
            downloaded += len(chunk)
            if total:
                pct = int(downloaded / total * 100)
                if pct != last_pct:
                    print(f"\r  {downloaded / 1e6:.1f} / {total / 1e6:.1f} MB ({pct}%)", end="", flush=True)
                    last_pct = pct
        print()
        data = b"".join(chunks)

    print(f"Extracting to {output_dir} ...")
    os.makedirs(output_dir, exist_ok=True)
    with zipfile.ZipFile(io.BytesIO(data)) as zf:
        zf.extractall(output_dir)
        names = zf.namelist()

    print(f"Extracted {len(names)} file(s):")
    for name in names:
        path = os.path.join(output_dir, name)
        size_mb = os.path.getsize(path) / 1e6
        print(f"  {name} ({size_mb:.1f} MB)")


if __name__ == "__main__":
    dir_path = os.path.dirname(os.path.realpath(__file__))
    output_dir = os.path.join(dir_path, "..", "data")
    if len(sys.argv) > 1:
        output_dir = sys.argv[1]
    main(output_dir=output_dir)
