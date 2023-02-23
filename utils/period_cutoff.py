# Determine which competitions span 2 "weeks"
# given a day of the week in which weeks are split

import os
import csv
from datetime import datetime, timedelta


def next_weekday(day, day_of_week=0):
    days_ahead = (day_of_week - day.weekday() + 7) % 7
    return day + timedelta(days_ahead)


# 0 is Monday
def main(day_of_week=0):

    dir_path = os.path.dirname(os.path.realpath(__file__))

    # Map competitions to start and day dates
    competitions = {}
    with open(f"{dir_path}/../data/WCA_export_Competitions.tsv") as f:
        rd = csv.reader(f, delimiter="\t")
        first = True
        for row in rd:
            if first:
                first = False
                continue
            start_year = int(row[5])
            start_month = int(row[6])
            start_day = int(row[7])
            end_month = int(row[8])
            end_day = int(row[9])
            end_year = start_year + 1 if start_month > end_month else start_year
            competitions[row[0]] = (
                datetime(start_year, start_month, start_day),
                datetime(end_year, end_month, end_day),
            )
    comp_lst = []
    for comp, dates in competitions.items():
        # Check if a competition day that is not the last is on the weekday
        cur_date = dates[0]
        while cur_date < dates[1]:
            if cur_date.weekday() == day_of_week:
                comp_lst.append(comp)
                break
            cur_date += timedelta(1)
    print(comp_lst)
    print(len(comp_lst))
    # Map period end dates to competitions
    periods = {}
    for comp, dates in competitions.items():
        period = next_weekday(dates[1], day_of_week)
        if period not in periods:
            periods[period] = set()
        periods[period].add(comp)
    print(periods)
    print(len(periods))
    # Label periods with distance from first period
    first_period = min(periods.keys())
    normed_periods = {}
    for period in periods:
        normed_periods[(period - first_period).days // 7] = periods[period]
    print(normed_periods)
    # Write into file
    with open(f"{dir_path}/../data/periods.tsv", "w", encoding="utf-8") as f:
        for period, comps in sorted(normed_periods.items()):
            f.write(f"{period}")
            for comp in comps:
                f.write(f"\t{comp}")
            f.write("\n")


if __name__ == "__main__":
    main(day_of_week=0)
