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

struct Result {
  std::string id;
  uintf period, dnfs, n;
  float *times;
};

// Read result from tsv
// Not applicable to MBLD
void get_results(std::ifstream &fs, std::vector<Result> &results) {
  std::string line;
  uintf n;
  // Number of results
  fs >> n;
  std::cerr << n << '\n';
  for (uintf i = 0; i < n; ++i) {
    std::string id;
    uintf period, dnfs, num;
    fs >> id >> period >> dnfs >> num;
    float *times = new float[num];
    for (uintf j = 0; j < num; ++j) {
      uintf temp;
      fs >> temp;
      times[j] = (float)temp / 100.0;
    }
    results.emplace_back(id, period, dnfs, n, times);
  }
}

void process_results(const std::vector<Result> &results,
                     std::vector<std::pair<std::string, Person>> &ratings,
                     float start_sigma2, float start_rho, float start_nu2,
                     float sigma2_const, float nu2_const, float min_rho,
                     float min_sigma2, float min_nu2) {
  System system{start_sigma2, start_rho, start_nu2,  sigma2_const,
                nu2_const,    min_rho,   min_sigma2, min_nu2};
  std::unordered_map<std::string, Person> people;
  for (uintf i = 0; i < results.size(); ++i) {
    if (i % 1000 == 0) {
      std::cerr << i << " results processed. " << results.size() - i << " to go!\n";
    }
    if (people.contains(results[i].id)) {
      people[results[i].id].update_stats(results[i].period, results[i].n,
                                         results[i].times);
    } else {
      people.emplace(results[i].id, Person{results[i].period, &system,
                                           results[i].n, results[i].times});
    }
  }

  for (auto &it : people) {
    ratings.push_back(it);
  }
}

int main() {
  std::vector<Result> results;
  std::ifstream fs("../data/period_results_333.tsv", std::ifstream::in);
  get_results(fs, results);
  std::cerr << "Completed get_results!\n";
  std::vector<std::pair<std::string, Person>> ratings;
  process_results(results, ratings, 3.0, 0.2, 0.05, 0.1, 0.01, 0.01, 0.001,
                  0.0001);
  for (uintf i = 0; i < ratings.size(); ++i) {
    std::cout << ratings[i].first << '\t' << ratings[i].second.get_mu() << '\n';
  }
  for (uintf i = 0; i < results.size(); ++i) {
    delete[] results[i].times;
  }
}