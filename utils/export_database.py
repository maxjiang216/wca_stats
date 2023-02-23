import requests, zipfile, io
import os


def main(path, chunk_size=128):
    url = "https://www.worldcubeassociation.org/results/misc/WCA_export.tsv.zip"
    r = requests.get(url)
    with open(path, "wb") as fd:
        for chunk in r.iter_content(chunk_size=chunk_size):
            fd.write(chunk)


if __name__ == "__main__":
    dir_path = os.path.dirname(os.path.realpath(__file__))
    main(path=f"{dir_path}/../data/export.zip")
