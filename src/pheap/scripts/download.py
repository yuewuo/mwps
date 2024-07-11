import argparse
import os
import sys
import urllib.request
import shutil
import gzip


dimacs_distance = {
    "dimacs-usa": [
        "http://www.diag.uniroma1.it//~challenge9/data/USA-road-d/USA-road-d.USA.gr.gz",
        "dimacs-usa.gr.gz",
        "dimacs-usa.gr",
    ],
    "dimacs-ctr": [
        "http://www.diag.uniroma1.it//~challenge9/data/USA-road-d/USA-road-d.CTR.gr.gz",
        "dimacs-ctr.gr.gz",
        "dimacs-ctr.gr"
    ],
    "dimacs-w": [
        "http://www.diag.uniroma1.it//~challenge9/data/USA-road-d/USA-road-d.W.gr.gz",
        "dimacs-w.gr.gz",
        "dimacs-w.gr"
    ],
    "dimacs-e": [
        "http://www.diag.uniroma1.it//~challenge9/data/USA-road-d/USA-road-d.E.gr.gz",
        "dimacs-e.gr.gz",
        "dimacs-e.gr"
    ],
    "dimacs-lks": [
        "http://www.diag.uniroma1.it//~challenge9/data/USA-road-d/USA-road-d.LKS.gr.gz",
        "dimacs-lks.gr.gz",
        "dimacs-lks.gr"
    ],
    "dimacs-cal": [
        "http://www.diag.uniroma1.it//~challenge9/data/USA-road-d/USA-road-d.CAL.gr.gz",
        "dimacs-cal.gr.gz",
        "dimacs-cal.gr"
    ],
    "dimacs-ne": [
        "http://www.diag.uniroma1.it//~challenge9/data/USA-road-d/USA-road-d.NE.gr.gz",
        "dimacs-ne.gr.gz",
        "dimacs-ne.gr"
    ],
    "dimacs-nw": [
        "http://www.diag.uniroma1.it//~challenge9/data/USA-road-d/USA-road-d.NW.gr.gz",
        "dimacs-nw.gr.gz",
        "dimacs-nw.gr"
    ],
    "dimacs-fla": [
        "http://www.diag.uniroma1.it//~challenge9/data/USA-road-d/USA-road-d.FLA.gr.gz",
        "dimacs-fla.gr.gz",
        "dimacs-fla.gr"
    ],
    "dimacs-col": [
        "http://www.diag.uniroma1.it//~challenge9/data/USA-road-d/USA-road-d.COL.gr.gz",
        "dimacs-col.gr.gz",
        "dimacs-col.gr"
    ],
    "dimacs-bay": [
        "http://www.diag.uniroma1.it//~challenge9/data/USA-road-d/USA-road-d.BAY.gr.gz",
        "dimacs-bay.gr.gz",
        "dimacs-bay.gr"
    ],
    "dimacs-ny": [
        "http://www.diag.uniroma1.it//~challenge9/data/USA-road-d/USA-road-d.NY.gr.gz",
        "dimacs-ny.gr.gz",
        "dimacs-ny.gr"
    ],
}


def download_dimacs(dataset):
    url_dl, gz_name, filename = dimacs_distance[dataset]
    gz_dest = os.path.join(args.dest, gz_name)
    file_dest = os.path.join(args.dest, filename)
    
    if not os.path.exists(file_dest) or not os.path.isfile(file_dest):
        print("> Download file: {}...".format(url_dl))

        urllib.request.urlretrieve(url_dl, gz_dest)

        with gzip.open(gz_dest, 'rb') as f_in:
            with open(file_dest, 'wb') as f_out:
                shutil.copyfileobj(f_in, f_out)

    print("> Dataset is ready at {}".format(file_dest))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Download datasets.")
    parser.add_argument('-d', '--dataset', required=True, help="Dataset to be downloaded", metavar="Dataset")
    parser.add_argument('--dest', required=True, help="Directory to store files", metavar="Dir")
    
    args = parser.parse_args()
    
    if not os.path.exists(args.dest):
        os.makedirs(args.dest)

    ds = args.dataset
    if ds.startswith("dimacs"):
        if ds == "dimacs-all":
            for k in dimacs_distance:
                download_dimacs(k)
        else:
            download_dimacs(ds)
        
