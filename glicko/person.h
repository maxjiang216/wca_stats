#ifndef PERSON_H
#define PERSON_H

#include "system.h"
#include "utils.h"

// Data structure representing a competitor in the WCA

class Person {
public:
  uintf last_competed;
  double mu, sigma2, rho, nu2;
  System *system;
  bool is_initialized;

  void initialize(uintf n, double *times);
  void update_rho(double xbar, double cur_nu2, uintf n, double *times);
  void update_sigma2(double xbar, double cur_sigma2, uintf n);
  void update_mu(double cur_sigma2, double xsum, double xbar, uintf n);
  void update_nu2(double cur_nu2, uintf n, double *times);

public:
  Person() = default;
  Person(System *system);
  ~Person() = default;

  void update_stats(uintf period, uintf n, double *times);

  // Accessors
  double get_mu() const;
};

#endif