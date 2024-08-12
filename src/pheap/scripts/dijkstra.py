import networkx as nx
import time

def dijkstra(filepath, runs):
    print("> Parse file: {}".format(filepath))
    G = nx.Graph()

    with open(filepath, 'r') as fin:
        lines = fin.readlines()

        for line in lines:
            if not line.startswith('a'):
                continue
            arr = line.strip().split()
            id1 = int(arr[1])
            id2 = int(arr[2])
            dist = int(arr[3])

            G.add_weighted_edges_from([(id1, id2, dist)])

    print("> Number of nodes: {}".format(len(G.nodes)))
    print("> Number of edges: {}".format(len(G.edges)))

    for irun in range(runs):
        start = time.time()
        a = nx.single_source_dijkstra(G, 10000, 1)
        end = time.time() - start
        print("Run {}: {} seconds".format(irun, end))
        print(a)

if __name__ == "__main__":
    0