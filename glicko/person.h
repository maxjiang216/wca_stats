#ifndef PERSON_H
#define PERSON_H

#include "system.h"
#include "utils.h"

// Data structure representing a competitor in the WCA

class Person {

  uintf last_competed;
  float mu, sigma2, rho, nu2;
  System *system;
  bool is_initialized;

  void initialize(uintf n, float *times);
  void update_rho(float xbar, float cur_nu2, uintf n, float *times);
  void update_sigma2(float xbar, float cur_sigma2, uintf n);
  void update_mu(float cur_sigma2, float xsum, float xbar);
  void update_nu2(float cur_nu2, uintf n, float *times);

public:
  Person() = default;
  Person(uintf period, System *system, uintf n, float *times);
  ~Person() = default;

  void update_stats(uintf period, uintf n, float *times);

  // Accessors
  float get_mu() const;
};

#endif