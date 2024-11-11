
# How to include image in README.md

The most elegant way I found is to follow this post: [https://stackoverflow.com/a/42677655](https://stackoverflow.com/a/42677655).
At specific version, the image referred to is a fixed link.


# How to create a Jupyter notebook server in a docker for public access (collaboration)

```sh
docker pull ubuntu:latest
sudo docker run -itd --name mwpf-jupyter-server -p 8888:8888 -v /home/ubuntu/mwpf-jupyter/:/home/mwpf-jupyter --restart always ubuntu:latest
sudo docker exec -it mwpf-jupyter-server bash

# inside the bash
apt install python3 python3-pip wget vim -y
#     install conda
mkdir -p ~/miniconda3
wget https://repo.anaconda.com/miniconda/Miniconda3-latest-Linux-x86_64.sh -O ~/miniconda3/miniconda.sh
bash ~/miniconda3/miniconda.sh -b -u -p ~/miniconda3
rm ~/miniconda3/miniconda.sh
source ~/miniconda3/bin/activate
#     install MWPF packages
pip3 install MWPF MWPF-rational --upgrade --force-reinstall
pip3 install jupyter jupyter-collaboration==2.1.5  # v3 has bug...
#     configure Jupyter lab to have consistent password every boot
jupyter lab --generate-config
vim ~/.jupyter/jupyter_lab_config.py  # modify it the following lines
# c.ServerApp.ip = '0.0.0.0'  # Allow access from any IP
# c.ServerApp.port = 8888  # Port to run on
# c.ServerApp.open_browser = False  # Do not open a browser on the server
# c.ServerApp.custom_display_url = 'jupyter.mwpf.dev'
# c.ServerApp.allow_root = True
jupyter notebook password  # set up password
#     create script to start the server automatically
vim /usr/local/bin/start-jupyter.sh  # put the following
# #!/bin/bash
# source ~/miniconda3/bin/activate
# jupyter lab --notebook-dir=/home/mwpf-jupyter
chmod +x /usr/local/bin/start-jupyter.sh

# on host machine, run the following to start the server if it's not already started
#     we'll still need a mechanism to automatically boot it
sudo docker exec -d mwpf-jupyter-server /bin/bash -c /usr/local/bin/start-jupyter.sh
```
