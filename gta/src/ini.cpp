#include "ini.h"
#include <algorithm>
#include <cctype>
#include <cstdlib>
#include <fstream>
#include <sstream>

static std::string lower(std::string s) {
	std::transform(s.begin(), s.end(), s.begin(), [](unsigned char c) { return char(std::tolower(c)); });
	return s;
}

static std::string trim(const std::string& s) {
	size_t a = s.find_first_not_of(" \t\r\n"), b = s.find_last_not_of(" \t\r\n");
	return a == std::string::npos ? "" : s.substr(a, b - a + 1);
}

bool Ini::loadFile(const std::string& path) {
	std::ifstream f(path, std::ios::binary);
	if (!f) return false;
	std::stringstream ss;
	ss << f.rdbuf();
	loadText(ss.str());
	return true;
}

void Ini::loadText(const std::string& text) {
	values_.clear();
	std::istringstream in(text);
	std::string line, section;
	while (std::getline(in, line)) {
		size_t c = line.find_first_of(";#");
		line = trim(c == std::string::npos ? line : line.substr(0, c));
		if (line.empty()) continue;
		if (line.front() == '[' && line.back() == ']') {
			section = lower(trim(line.substr(1, line.size() - 2)));
			continue;
		}
		size_t eq = line.find('=');
		if (eq == std::string::npos) continue;
		values_[section + "." + lower(trim(line.substr(0, eq)))] = trim(line.substr(eq + 1));
	}
}

bool Ini::has(const std::string& s, const std::string& k) const { return values_.count(lower(s) + "." + lower(k)) != 0; }

std::string Ini::str(const std::string& s, const std::string& k, const std::string& def) const {
	auto it = values_.find(lower(s) + "." + lower(k));
	return it == values_.end() ? def : it->second;
}

double Ini::num(const std::string& s, const std::string& k, double def) const {
	std::string v = str(s, k, "");
	if (v.empty()) return def;
	char* end = nullptr;
	double d = std::strtod(v.c_str(), &end);
	return (end && *end == '\0') ? d : def;
}

float Ini::numf(const std::string& s, const std::string& k, float def) const {
	std::string v = str(s, k, "");
	if (v.empty()) return def;
	char* end = nullptr;
	float f = std::strtof(v.c_str(), &end);
	return (end && *end == '\0') ? f : def;
}

bool Ini::flag(const std::string& s, const std::string& k, bool def) const {
	std::string v = lower(str(s, k, ""));
	if (v == "1" || v == "true" || v == "on" || v == "yes") return true;
	if (v == "0" || v == "false" || v == "off" || v == "no") return false;
	return def;
}
