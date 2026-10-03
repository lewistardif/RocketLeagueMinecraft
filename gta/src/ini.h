#pragma once
#include <map>
#include <string>

class Ini {
public:
	bool loadFile(const std::string& path);
	void loadText(const std::string& text);
	bool has(const std::string& section, const std::string& key) const;
	std::string str(const std::string& section, const std::string& key, const std::string& def) const;
	double num(const std::string& section, const std::string& key, double def) const;
	float numf(const std::string& section, const std::string& key, float def) const;
	bool flag(const std::string& section, const std::string& key, bool def) const;

private:
	std::map<std::string, std::string> values_;
};
