"""What the chain measures and an instrument keeps (usage: BRASS=<name> chain_fit.py gain): the output so that a loud note's peaks stand at about the soft
limit. Writes inst/<name>/spec.json, which spec.py reads."""
import multiprocessing
import numpy as np
from concurrent.futures import ProcessPoolExecutor
import spec



def peak(k):
    import live
    x = live.render_events([(0.05, "on", k, 127), (0.9, "off", k)], 1.0, "open", True)
    return float(np.abs(x).max())


if __name__ == "__main__":
    if True:
        import live, rt_tables
        rt_tables.load("open")
        with ProcessPoolExecutor(10, mp_context=multiprocessing.get_context("fork")) as ex:
            p = np.array(list(ex.map(peak, rt_tables.TABLE)))
        gain = round(float(-20 * np.log10(np.median(p) * live.GAIN)), 1)
        spec.save("gain_db", gain)
        print(spec.NAME, "gain", gain, "dB; v127 peaks after it, median 1, largest", round(float(p.max() / np.median(p)), 2))
