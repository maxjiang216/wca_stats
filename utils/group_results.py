# We group all results for one event to a period
# Labelled by period_cutoff.py

import os
import csv

# Return a list of results from the row given
def process_result(row, is_mbld=False):
    res = []
    for item in row[10:15]:
        time = int(item)
        # Filter DNS and no result
        if time not in [-2, 0]:
            # Keep results non-negative
            if time < 0:
                time = 0
            res.append(time)
    return res


def main(event="333"):

    dir_path = os.path.dirname(os.path.realpath(__file__))

    # Map competitions to periods
    comps = {}
    with open(f"{dir_path}/../data/periods.tsv") as f:
        rd = csv.reader(f, delimiter="\t")
        for row in rd:
            for comp in row[1:]:
                comps[comp] = row[0]

    # Group results by periods, then by person
    periods = {}
    with open(f"{dir_path}/../data/WCA_export_Results.tsv") as f:
        rd = csv.reader(f, delimiter="\t")
        first = True
        for row in rd:
            if first:
                first = False
                continue
            event_id = row[1]
            if event_id != event:
                continue
            competition = row[0]
            period = comps[competition]
            if period not in periods:
                periods[period] = {}
            id = row[7]
            if id not in periods[period]:
                periods[period][id] = []
            periods[period][id].extend(process_result(row))

    # Filter data more
    period_person = {}
    for period, people in periods.items():
        for person, times in people.items():
            times.sort()
            dnfs = times.count(0)
            period_person[(period, person)] = (dnfs, times[dnfs:])

    # Write data to file
    with open(
        f"{dir_path}/../data/period_results_{event}.tsv", "w", encoding="utf-8"
    ) as f:
        f.write(f"{len(period_person)}\n")
        max_res = 0
        best_avg = 0
        max_id_period = None
        for k, res in sorted(period_person.items()):
            if len(res[1]) > max_res or len(res[1]) == max_res and sum(res[1]) < best_avg:
                max_res = len(res[1])
                best_avg = sum(res[1])
                max_id_period = k
            f.write(f"{k[1]}\t{k[0]}\t{res[0]}\t{len(res[1])}")
            for time in res[1]:
                f.write(f"\t{time}")
            f.write("\n")
    print(max_id_period, max_res, best_avg / max_res)


if __name__ == "__main__":
    main(event="333")
