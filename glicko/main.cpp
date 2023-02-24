#include "person.h"
#include "system.h"
#include "utils.h"
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>
#include <cmath>

struct Result {
  std::string id;
  uintf period, dnfs, n;
  double *times;
};

// Read result from tsv
// Not applicable to MBLD
void get_results(std::ifstream &fs, std::vector<std::vector<Result>> &results,
                 std::unordered_map<std::string, Person> &people,
                 System *system) {
  std::string line;
  uintf n;
  // Number of results
  fs >> n;
  std::cerr << n << '\n';
  uintf cur_period = 1;
  for (uintf i = 0; i < n; ++i) {
    std::string id;
    uintf period, dnfs, num;
    fs >> id >> period >> dnfs >> num;
    if (num == 0)
      continue;
    if (period != cur_period) {
      cur_period = period;
      results.emplace_back();
    }
    double *times = new double[num];
    for (uintf j = 0; j < num; ++j) {
      uintf temp;
      fs >> temp;
      times[j] = log((double)temp / 100.0);
    }
    results[results.size() - 1].emplace_back(id, period, dnfs, num, times);
    people[id] = Person{system};
  }
}

void process_results(const std::vector<std::vector<Result>> &results,
                     std::unordered_map<std::string, Person> &people,
                     std::vector<std::pair<std::string, Person>> &ratings) {
  for (uintf i = 0; i < results.size(); ++i) {
    if (i % 10 == 0) {
      std::cerr << i << " results processed. " << results.size() - i
                << " to go!\n";
    }
    //#pragma omp parallel for
    for (uintf j = 0; j < results[i].size(); ++j) {
      people[results[i][j].id].update_stats(
          results[i][j].period, results[i][j].n, results[i][j].times);
          if (results[i][j].id == "2016KOLA02") {
            std::cerr << exp(people[results[i][j].id].mu) << ' ' << people[results[i][j].id].sigma2 << ' ' << people[results[i][j].id].rho << ' ' << people[results[i][j].id].nu2 << '\n';
          }
    }
  }

  std::cerr << "Done processing results!\n";

  for (auto &it : people) {
    ratings.push_back(it);
  }
}

int main() {
  double start_sigma2 = 10.0, start_rho = 0.2, start_nu2 = 0.1,
         sigma2_const = 0.55, nu2_const = 0.03, min_rho = 0.001,
         min_sigma2 = 1e-8, min_nu2 = 0.0;
  System *system = new System{start_sigma2, start_rho, start_nu2,  sigma2_const,
                              nu2_const,    min_rho,   min_sigma2, min_nu2};
  std::vector<std::vector<Result>> results;
  std::unordered_map<std::string, Person> people;
  std::vector<std::pair<std::string, Person>> ratings;
  std::ifstream fs("../data/period_results_333.tsv", std::ifstream::in);
  get_results(fs, results, people, system);
  std::cerr << "Completed get_results!\n";
  process_results(results, people, ratings);
  std::cerr << "Completed process_results!\n";
  for (uintf i = 0; i < ratings.size(); ++i) {
    if (!ratings[i].second.is_initialized || exp(ratings[i].second.mu) > 7.0) {
      continue;
    }
    std::cout << ratings[i].first << '\t' << exp(ratings[i].second.get_mu()) << '\t'
              << ratings[i].second.last_competed << ' ' << ratings[i].second.rho
              << ' ' << ratings[i].second.sigma2 << ' ' << ratings[i].second.nu2
              << '\n';
  }
  for (uintf i = 0; i < results.size(); ++i) {
    for (uintf j = 0; j < results[i].size(); ++j) {
      delete[] results[i][j].times;
    }
  }
  delete system;
}